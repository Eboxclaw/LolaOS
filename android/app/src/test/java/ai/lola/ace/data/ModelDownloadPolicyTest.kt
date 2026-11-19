package ai.lola.ace.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.fail
import org.junit.Test

class ModelDownloadPolicyTest {
    private val artifact = ModelArtifactSummary(
        id = "liquidai.lfm3-1.7b.gguf.q4_k_m",
        displayName = "LFM3-1.7B · Q4_K_M",
        family = "LFM3",
        publisher = "Liquid AI",
        sourceRepository = "LiquidAI/LFM3-1.7B-GGUF",
        revision = "b3f2ae5d73cd3c91bf40e8ba752c648aa05dffdf",
        fileName = "LFM3-1.7B-Q4_K_M.gguf",
        format = "gguf",
        quantization = "Q4_K_M",
        downloadUrl = "https://huggingface.co/LiquidAI/LFM3-1.7B-GGUF/resolve/b3f2ae5d73cd3c91bf40e8ba752c648aa05dffdf/LFM3-1.7B-Q4_K_M.gguf",
        sha256 = "3793e1e6e2266919d6e015287221b41100175a98eddaba92e111aed3b05c93c4",
        sizeLabel = "1.1 GB (publisher-rounded)",
        advertisedContext = "256K tokens",
        architecture = "LFM3",
        license = "LFM Open License v1.0",
    )

    @Test
    fun acceptsPinnedOfficialGgufArtifact() {
        ModelDownloadPolicy.validateArtifact(artifact)
    }

    @Test
    fun rejectsMutableModelRevision() {
        expectIllegalArgument {
            ModelDownloadPolicy.validateArtifact(artifact.copy(revision = "main"))
        }
    }

    @Test
    fun rejectsUrlThatDoesNotMatchPinnedArtifact() {
        expectIllegalArgument {
            ModelDownloadPolicy.validateArtifact(
                artifact.copy(downloadUrl = "https://example.com/model.gguf"),
            )
        }
    }

    @Test
    fun rejectsPathTraversalFilename() {
        expectIllegalArgument {
            ModelDownloadPolicy.validateArtifact(artifact.copy(fileName = "../model.gguf"))
        }
    }

    @Test
    fun rejectsUnsupportedArtifactFormat() {
        expectIllegalArgument {
            ModelDownloadPolicy.validateArtifact(
                artifact.copy(format = "onnx", fileName = "model.onnx"),
            )
        }
    }

    @Test
    fun parsesValidPartialContentRange() {
        val range = ModelDownloadPolicy.parseContentRange("bytes 1048576-2097151/4194304")

        assertNotNull(range)
        assertEquals(1_048_576L, range!!.start)
        assertEquals(2_097_151L, range.endInclusive)
        assertEquals(4_194_304L, range.totalBytes)
    }

    @Test
    fun parsesPartialContentRangeWithUnknownTotal() {
        val range = ModelDownloadPolicy.parseContentRange("bytes 100-199/*")

        assertNotNull(range)
        assertEquals(100L, range!!.start)
        assertEquals(199L, range.endInclusive)
        assertNull(range.totalBytes)
    }

    @Test
    fun rejectsMalformedOrInconsistentContentRange() {
        assertNull(ModelDownloadPolicy.parseContentRange("items 0-99/100"))
        assertNull(ModelDownloadPolicy.parseContentRange("bytes 100-99/200"))
        assertNull(ModelDownloadPolicy.parseContentRange("bytes 0-200/200"))
        assertNull(ModelDownloadPolicy.parseContentRange("bytes x-y/z"))
    }

    @Test
    fun parsesUnsatisfiedRangeTotal() {
        assertEquals(4_194_304L, ModelDownloadPolicy.parseUnsatisfiedRangeTotal("bytes */4194304"))
        assertNull(ModelDownloadPolicy.parseUnsatisfiedRangeTotal("bytes */unknown"))
    }

    private inline fun expectIllegalArgument(block: () -> Unit) {
        try {
            block()
            fail("Expected IllegalArgumentException")
        } catch (_: IllegalArgumentException) {
            // Expected.
        }
    }
}
