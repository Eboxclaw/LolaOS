//! The lola-custom engine — **our own inference implementation specialized
//! for LFM3 tri-hybrid architectures** (ADR-003 rev., `docs/ENGINE.md`).
//!
//! Nothing from llama.cpp ships inside LOLA. llama.cpp serves exactly two
//! external roles: correctness oracle (golden outputs) and GGUF import
//! source. The kernels, the weight layout and the dual-slot scheduler below
//! are the product.
//!
//! LFM3 reference facts (verified against Liquid AI's published configs —
//! see ENGINE.md sources): 24 layers = 13 gated short convolutions
//! (kernel 7) + 5 sliding-window attention (window 256) + 6 global
//! attention layers (GQA 16/4 heads × 128 dim, partial RoPE on 50% of
//! head_dim at θ=1e6; SWA layers keep full RoPE at θ=1e4), head-wise
//! sigmoid gates, sandwich norms, 128K vocab, tied embeddings, MLP
//! intermediate 7680.

use serde::{Deserialize, Serialize};

/// Version of the lola-native weight layout produced at install time.
/// Bumping it invalidates every repacked model.
pub const LOLA_WEIGHT_LAYOUT_VERSION: u16 = 1;

/// SWA window size (tokens) — a ring buffer, not a growing KV.
pub const SWA_WINDOW: u32 = 256;
/// GQA shape of global attention layers.
pub const GQA_QUERY_HEADS: u32 = 16;
pub const GQA_KV_HEADS: u32 = 4;
pub const HEAD_DIM: u32 = 128;
/// Fraction of head_dim rotated by RoPE on global layers (partial RoPE).
pub const PARTIAL_ROPE_FRACTION: f64 = 0.5;
pub const GLOBAL_ROPE_THETA: f64 = 1.0e6;
pub const SWA_ROPE_THETA: f64 = 1.0e4;
/// Gated short-conv kernel size.
pub const CONV_KERNEL: u32 = 7;

/// The kernel inventory. Every variant is implemented by us; a kernel may
/// serve the engine only after its oracle-parity gate passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineOp {
    /// Gated short convolution, kernel 7, constant-size streaming state.
    GatedConv7,
    /// Sliding-window attention over the [`SWA_WINDOW`] ring buffer.
    SwaAttention,
    /// Global attention, GQA 16/4, partial RoPE, KV in unified cache.
    GlobalAttention,
    /// Head-wise sigmoid gate on attention output (near-free, fused).
    HeadGate,
    /// Sandwich norm (pre + post per block).
    SandwichNorm,
    /// Quantized matmul family (per-tensor quant mixes).
    Q4Matmul,
    /// Tied-embedding lookup + logits head.
    TiedEmbedding,
    /// Gated MLP, intermediate width 7680.
    GatedMlp,
    /// d1 typed-answer readout (ported from `modeling_d1_lfm3_*.py`).
    D1Readout,
    /// LiquidCodec causal audio encoder — LFM3-block streaming encoder
    /// (K1–K3 kernels reused); ~361× realtime on one CPU thread.
    CausalAudioEncoder,
    /// GFSQ audio-token prediction — parallel multi-token refinement over
    /// independent codebook groups (not a long autoregressive stream).
    GfsqPredictor,
    /// Direct spectral-prediction decoder — LFM3 blocks; decoder-only QAT
    /// Q4_0 build is 3.5× smaller with minimal quality loss.
    SpectralDecoder,
}

impl EngineOp {
    /// The engine milestone that first ships this kernel (K0–K8).
    pub fn milestone(self) -> KernelMilestone {
        match self {
            Self::GatedConv7 => KernelMilestone::K1ConvKernel,
            Self::SwaAttention => KernelMilestone::K2SwaAttention,
            Self::GlobalAttention => KernelMilestone::K3GlobalAttention,
            Self::HeadGate | Self::SandwichNorm => KernelMilestone::K4GateNormFusion,
            Self::D1Readout => KernelMilestone::K5D1Readout,
            Self::CausalAudioEncoder | Self::GfsqPredictor | Self::SpectralDecoder => {
                KernelMilestone::K9CodecPath
            }
            Self::TiedEmbedding | Self::GatedMlp | Self::Q4Matmul => {
                KernelMilestone::K0WeightsContainer
            }
        }
    }
}

/// Layer taxonomy of the tri-hybrid backbone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerKind {
    GatedConv,
    SlidingAttention,
    GlobalAttention,
}

/// One block of the backbone, as our graph builder sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerSpec {
    pub index: u32,
    pub kind: LayerKind,
}

/// The LFM3-1.7B-class backbone shape: 13 conv + 5 SWA + 6 global = 24
/// layers. The authoritative per-layer order is read from GGUF metadata at
/// import time (K0); this builder expresses the canonical shape with
/// attention layers spread at fixed intervals.
pub fn lfm3_backbone() -> Vec<LayerSpec> {
    let mut kinds = [LayerKind::GatedConv; 24];
    // Global attention every 4th layer (positions 4, 8, 12, 16, 20, 24).
    for i in [3, 7, 11, 15, 19, 23] {
        kinds[i] = LayerKind::GlobalAttention;
    }
    // SWA between them (positions 2, 6, 10, 14, 18).
    for i in [1, 5, 9, 13, 17] {
        kinds[i] = LayerKind::SlidingAttention;
    }
    kinds
        .iter()
        .enumerate()
        .map(|(index, kind)| LayerSpec {
            index: index as u32 + 1,
            kind: *kind,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Weights pipeline: GGUF interchange → lola-native layout
// ---------------------------------------------------------------------------

/// Where weights come from. Official Liquid GGUFs, unmodified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GgufSource {
    pub repository: String,
    pub file: String,
    pub sha256: String,
}

/// Per-tensor-class quantization assignment in the repacked layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TensorClass {
    AttentionProj,
    ConvWeights,
    MlpWeights,
    Embeddings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuantKind {
    Q4_0,
    Q4kM,
    Q6k,
    Q8_0,
    Bf16,
}

/// How a checkpoint's quantization came to exist — a weights-pipeline
/// fact, not a format: a QAD checkpoint dequantizes as plain Q4_0.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuantProvenance {
    /// Post-training quantization (default).
    #[default]
    Ptq,
    /// Quantization-aware distillation (teacher → quantized student);
    /// claimed ~97% of BF16 — LFM3 QAD has no published evals, so our
    /// SB golden gates arbitrate before slot-A use.
    Qad,
    /// Quantization-aware training (e.g. LiquidCodec decoder Q4_0).
    Qat,
    FullPrecision,
}

/// One rung of the owner's quant ladder, with its provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QuantChoice {
    pub kind: QuantKind,
    pub provenance: QuantProvenance,
}

/// Owner rule (ADR-040): **QAD-Q4_0 → Q4_K_M → Q8_0.** Given the quants
/// a checkpoint actually ships (with provenance), return the ladder's
/// pick: a QAD-flagged Q4_0 wins; otherwise the first Q4_K_M; otherwise
/// Q8_0; otherwise `None`.
pub fn preferred_quant(available: &[QuantChoice]) -> Option<QuantKind> {
    if let Some(choice) = available
        .iter()
        .find(|c| c.kind == QuantKind::Q4_0 && c.provenance == QuantProvenance::Qad)
    {
        return Some(choice.kind);
    }
    if let Some(choice) = available.iter().find(|c| c.kind == QuantKind::Q4kM) {
        return Some(choice.kind);
    }
    available
        .iter()
        .find(|c| c.kind == QuantKind::Q8_0)
        .map(|c| c.kind)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuantAssignment {
    pub tensor_class: TensorClass,
    pub quant: QuantKind,
}

/// The lola-native layout written at install time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LolaLayout {
    pub version: u16,
    /// Per-tensor-class quant mix (measured, not guessed — ENGINE.md §3).
    pub quant_mix: Vec<QuantAssignment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepackPolicy {
    /// Repack once at model-install time, never in the decision path.
    InstallTime,
}

/// The import plan for one model: official GGUF in, our layout out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeightImportPlan {
    pub model_id: String,
    pub source: GgufSource,
    pub layout: LolaLayout,
    pub repack: RepackPolicy,
}

impl WeightImportPlan {
    pub fn quant_for(&self, tensor_class: TensorClass) -> Option<QuantKind> {
        self.layout
            .quant_mix
            .iter()
            .find(|assignment| assignment.tensor_class == tensor_class)
            .map(|assignment| assignment.quant)
    }
}

// ---------------------------------------------------------------------------
// Engine milestones (ROADMAP S3): K0 → K8, strictly ordered
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelMilestone {
    K0WeightsContainer,
    K1ConvKernel,
    K2SwaAttention,
    K3GlobalAttention,
    K4GateNormFusion,
    K5D1Readout,
    K6DualSlotScheduler,
    K7VulkanWgpu,
    K8PluginAbi,
    /// LiquidCodec voice path: causal encoder → GFSQ parallel prediction →
    /// spectral decoder. Reuses K1–K3 tri-hybrid kernels (the codec's
    /// encoder and decoder are built from the same LFM3 blocks).
    K9CodecPath,
}

impl KernelMilestone {
    pub const ALL: [KernelMilestone; 10] = [
        Self::K0WeightsContainer,
        Self::K1ConvKernel,
        Self::K2SwaAttention,
        Self::K3GlobalAttention,
        Self::K4GateNormFusion,
        Self::K5D1Readout,
        Self::K6DualSlotScheduler,
        Self::K7VulkanWgpu,
        Self::K8PluginAbi,
        Self::K9CodecPath,
    ];

    pub fn stage(self) -> &'static str {
        match self {
            Self::K0WeightsContainer => "weights container + GGUF import",
            Self::K1ConvKernel => "gated conv k=7 kernel (streaming state)",
            Self::K2SwaAttention => "SWA-256 ring-buffer attention",
            Self::K3GlobalAttention => "global attention, partial RoPE, GQA 16/4",
            Self::K4GateNormFusion => "head-gate + sandwich-norm fusion",
            Self::K5D1Readout => "d1 typed-answer readout port",
            Self::K6DualSlotScheduler => "dual-slot scheduler (prefill burst + streaming)",
            Self::K7VulkanWgpu => "wgpu/Vulkan kernels (AMD, Tensor Mali)",
            Self::K8PluginAbi => "NPU plugin ABI for our graph",
            Self::K9CodecPath => "LiquidCodec voice path (encoder · GFSQ · decoder)",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backbone_matches_lfm3_shape() {
        let backbone = lfm3_backbone();
        assert_eq!(backbone.len(), 24);
        let convs = backbone
            .iter()
            .filter(|l| l.kind == LayerKind::GatedConv)
            .count();
        let swa = backbone
            .iter()
            .filter(|l| l.kind == LayerKind::SlidingAttention)
            .count();
        let global = backbone
            .iter()
            .filter(|l| l.kind == LayerKind::GlobalAttention)
            .count();
        assert_eq!((convs, swa, global), (13, 5, 6));
        // Only 6 of 24 layers carry long KV — the residency economics (ADR-024).
        assert_eq!(global, 6);
    }

    #[test]
    fn milestones_are_strictly_ordered() {
        for pair in KernelMilestone::ALL.windows(2) {
            assert!(pair[0] < pair[1], "milestones must strictly increase");
        }
    }

    #[test]
    fn every_op_maps_to_a_milestone() {
        assert_eq!(
            EngineOp::GatedConv7.milestone(),
            KernelMilestone::K1ConvKernel
        );
        assert_eq!(
            EngineOp::SwaAttention.milestone(),
            KernelMilestone::K2SwaAttention
        );
        assert_eq!(
            EngineOp::GlobalAttention.milestone(),
            KernelMilestone::K3GlobalAttention
        );
        assert_eq!(
            EngineOp::D1Readout.milestone(),
            KernelMilestone::K5D1Readout
        );
        assert_eq!(
            EngineOp::Q4Matmul.milestone(),
            KernelMilestone::K0WeightsContainer
        );
    }

    #[test]
    fn quant_ladder_qad_first() {
        let available = vec![
            QuantChoice {
                kind: QuantKind::Q8_0,
                provenance: QuantProvenance::Ptq,
            },
            QuantChoice {
                kind: QuantKind::Q4_0,
                provenance: QuantProvenance::Qad,
            },
            QuantChoice {
                kind: QuantKind::Q4kM,
                provenance: QuantProvenance::Ptq,
            },
        ];
        assert_eq!(preferred_quant(&available), Some(QuantKind::Q4_0));
    }

    #[test]
    fn quant_ladder_falls_back_to_q4km_then_q8() {
        let no_qad = vec![
            QuantChoice {
                kind: QuantKind::Q4_0,
                provenance: QuantProvenance::Ptq,
            },
            QuantChoice {
                kind: QuantKind::Q4kM,
                provenance: QuantProvenance::Ptq,
            },
        ];
        assert_eq!(preferred_quant(&no_qad), Some(QuantKind::Q4kM));

        let q8_only = vec![
            QuantChoice {
                kind: QuantKind::Bf16,
                provenance: QuantProvenance::FullPrecision,
            },
            QuantChoice {
                kind: QuantKind::Q8_0,
                provenance: QuantProvenance::Ptq,
            },
        ];
        assert_eq!(preferred_quant(&q8_only), Some(QuantKind::Q8_0));

        assert_eq!(preferred_quant(&[]), None);
    }

    #[test]
    fn import_plan_resolves_quant_mixes() {
        let plan = WeightImportPlan {
            model_id: "lfm3-omni-2b".into(),
            source: GgufSource {
                repository: "LiquidAI/LFM3-Omni-2B-GGUF".into(),
                file: "LFM3-Omni-2B-Q4_K_M.gguf".into(),
                sha256: "ab".repeat(32),
            },
            layout: LolaLayout {
                version: LOLA_WEIGHT_LAYOUT_VERSION,
                quant_mix: vec![
                    QuantAssignment {
                        tensor_class: TensorClass::AttentionProj,
                        quant: QuantKind::Q6k,
                    },
                    QuantAssignment {
                        tensor_class: TensorClass::ConvWeights,
                        quant: QuantKind::Q8_0,
                    },
                ],
            },
            repack: RepackPolicy::InstallTime,
        };
        assert_eq!(
            plan.quant_for(TensorClass::AttentionProj),
            Some(QuantKind::Q6k)
        );
        assert_eq!(
            plan.quant_for(TensorClass::ConvWeights),
            Some(QuantKind::Q8_0)
        );
        assert_eq!(plan.quant_for(TensorClass::Embeddings), None);
        assert_eq!(plan.layout.version, 1);
    }
}
