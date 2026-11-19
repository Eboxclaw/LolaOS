#include <jni.h>
#include <android/log.h>

#include "llama.h"
#include "chat.h"
#include "common.h"
#include "sampling.h"

#include <algorithm>
#include <atomic>
#include <cstdint>
#include <chrono>
#include <cstring>
#include <exception>
#include <utility>
#include <mutex>
#include <string>
#include <vector>

namespace {

constexpr char LOG_TAG[] = "LOLA-ACE-Reference";
constexpr uint32_t DEFAULT_CONTEXT_TOKENS = 2048;
constexpr uint32_t MAX_CONTEXT_TOKENS = 8192;
constexpr int32_t PROMPT_BATCH_TOKENS = 512;
constexpr int32_t TOKEN_PIECE_BUFFER_BYTES = 256;

std::once_flag g_backend_init_once;
std::atomic<bool> g_backend_initialized{false};

struct ModelHandle {
    llama_model *model = nullptr;
    common_chat_templates_ptr chat_templates;
};

struct SessionHandle {
    ModelHandle *owner = nullptr;
    llama_context *context = nullptr;
    llama_batch_ext *batch = nullptr;
    common_sampler *sampler = nullptr;
    const llama_vocab *vocab = nullptr;
    uint32_t context_tokens = DEFAULT_CONTEXT_TOKENS;
    std::atomic<bool> cancelled{false};
    std::mutex generation_mutex;
};

template <typename T>
T *from_handle(jlong value) {
    return reinterpret_cast<T *>(static_cast<uintptr_t>(value));
}

template <typename T>
jlong to_handle(T *value) {
    return static_cast<jlong>(reinterpret_cast<uintptr_t>(value));
}

void throw_java(JNIEnv *env, const char *class_name, const std::string &message) {
    jclass klass = env->FindClass(class_name);
    if (klass != nullptr) {
        env->ThrowNew(klass, message.c_str());
        env->DeleteLocalRef(klass);
    }
}

void append_utf8(std::string &output, uint32_t codepoint) {
    if (codepoint <= 0x7f) {
        output.push_back(static_cast<char>(codepoint));
    } else if (codepoint <= 0x7ff) {
        output.push_back(static_cast<char>(0xc0 | (codepoint >> 6)));
        output.push_back(static_cast<char>(0x80 | (codepoint & 0x3f)));
    } else if (codepoint <= 0xffff) {
        output.push_back(static_cast<char>(0xe0 | (codepoint >> 12)));
        output.push_back(static_cast<char>(0x80 | ((codepoint >> 6) & 0x3f)));
        output.push_back(static_cast<char>(0x80 | (codepoint & 0x3f)));
    } else {
        output.push_back(static_cast<char>(0xf0 | (codepoint >> 18)));
        output.push_back(static_cast<char>(0x80 | ((codepoint >> 12) & 0x3f)));
        output.push_back(static_cast<char>(0x80 | ((codepoint >> 6) & 0x3f)));
        output.push_back(static_cast<char>(0x80 | (codepoint & 0x3f)));
    }
}

// JNI GetStringUTFChars returns Modified UTF-8, not standard UTF-8. Convert
// UTF-16 explicitly so supplementary characters and embedded NUL are valid input.
std::string jstring_to_utf8(JNIEnv *env, jstring value) {
    if (value == nullptr) return {};

    const jsize length = env->GetStringLength(value);
    const jchar *chars = env->GetStringChars(value, nullptr);
    if (chars == nullptr) return {};

    std::string output;
    output.reserve(static_cast<size_t>(length) * 3);
    for (jsize i = 0; i < length; ++i) {
        uint32_t codepoint = chars[i];
        if (codepoint >= 0xd800 && codepoint <= 0xdbff) {
            if (i + 1 < length && chars[i + 1] >= 0xdc00 && chars[i + 1] <= 0xdfff) {
                const uint32_t low = chars[++i];
                codepoint = 0x10000 + ((codepoint - 0xd800) << 10) + (low - 0xdc00);
            } else {
                codepoint = 0xfffd;
            }
        } else if (codepoint >= 0xdc00 && codepoint <= 0xdfff) {
            codepoint = 0xfffd;
        }
        append_utf8(output, codepoint);
    }

    env->ReleaseStringChars(value, chars);
    return output;
}

std::u16string utf8_to_utf16_complete(const std::string &input, size_t &consumed) {
    std::u16string output;
    size_t i = 0;
    while (i < input.size()) {
        const auto first = static_cast<uint8_t>(input[i]);
        uint32_t codepoint = 0;
        size_t width = 0;

        if (first <= 0x7f) {
            codepoint = first;
            width = 1;
        } else if ((first & 0xe0) == 0xc0) {
            codepoint = first & 0x1f;
            width = 2;
        } else if ((first & 0xf0) == 0xe0) {
            codepoint = first & 0x0f;
            width = 3;
        } else if ((first & 0xf8) == 0xf0) {
            codepoint = first & 0x07;
            width = 4;
        } else {
            output.push_back(static_cast<char16_t>(0xfffd));
            ++i;
            continue;
        }

        if (i + width > input.size()) {
            break;
        }

        bool valid = true;
        for (size_t j = 1; j < width; ++j) {
            const auto next = static_cast<uint8_t>(input[i + j]);
            if ((next & 0xc0) != 0x80) {
                valid = false;
                break;
            }
            codepoint = (codepoint << 6) | (next & 0x3f);
        }

        const bool overlong =
            (width == 2 && codepoint < 0x80) ||
            (width == 3 && codepoint < 0x800) ||
            (width == 4 && codepoint < 0x10000);
        if (!valid || overlong || codepoint > 0x10ffff ||
            (codepoint >= 0xd800 && codepoint <= 0xdfff)) {
            output.push_back(static_cast<char16_t>(0xfffd));
            ++i;
            continue;
        }

        if (codepoint <= 0xffff) {
            output.push_back(static_cast<char16_t>(codepoint));
        } else {
            codepoint -= 0x10000;
            output.push_back(static_cast<char16_t>(0xd800 + (codepoint >> 10)));
            output.push_back(static_cast<char16_t>(0xdc00 + (codepoint & 0x3ff)));
        }
        i += width;
    }
    consumed = i;
    return output;
}

bool emit_utf8(JNIEnv *env, jobject sink, jmethodID on_token, std::string &pending, bool flush) {
    size_t consumed = 0;
    std::u16string utf16 = utf8_to_utf16_complete(pending, consumed);

    if (flush && consumed < pending.size()) {
        utf16.push_back(static_cast<char16_t>(0xfffd));
        consumed = pending.size();
    }

    if (!utf16.empty()) {
        jstring chunk = env->NewString(
            reinterpret_cast<const jchar *>(utf16.data()),
            static_cast<jsize>(utf16.size()));
        if (chunk == nullptr) {
            return false;
        }
        env->CallVoidMethod(sink, on_token, chunk);
        env->DeleteLocalRef(chunk);
        if (env->ExceptionCheck()) {
            return false;
        }
    }

    if (consumed > 0) {
        pending.erase(0, consumed);
    }
    return true;
}

bool callback_cancelled(JNIEnv *env, jobject sink, jmethodID is_cancelled) {
    const jboolean cancelled = env->CallBooleanMethod(sink, is_cancelled);
    return env->ExceptionCheck() || cancelled == JNI_TRUE;
}

using SteadyClock = std::chrono::steady_clock;

int64_t elapsed_millis(SteadyClock::time_point start, SteadyClock::time_point end) {
    return std::chrono::duration_cast<std::chrono::milliseconds>(end - start).count();
}

void emit_metrics(
    JNIEnv *env,
    jobject sink,
    jmethodID on_metrics,
    int32_t prompt_tokens,
    int32_t generated_tokens,
    int64_t prefill_millis,
    int64_t ttft_millis,
    int64_t decode_millis,
    int64_t total_millis) {
    env->CallVoidMethod(
        sink,
        on_metrics,
        static_cast<jint>(prompt_tokens),
        static_cast<jint>(generated_tokens),
        static_cast<jlong>(prefill_millis),
        static_cast<jlong>(ttft_millis),
        static_cast<jlong>(decode_millis),
        static_cast<jlong>(total_millis));
}

bool decode_prompt(SessionHandle *session, const std::vector<llama_token> &tokens) {
    const size_t total = tokens.size();
    for (size_t start = 0; start < total; start += PROMPT_BATCH_TOKENS) {
        if (session->cancelled.load()) return false;
        const size_t count = std::min(
            static_cast<size_t>(PROMPT_BATCH_TOKENS),
            total - start);
        llama_batch_ext_clear(session->batch);

        for (size_t i = 0; i < count; ++i) {
            const int32_t batch_index = llama_batch_ext_add_token(
                session->batch, 0, tokens[start + i]);
            if (batch_index < 0) {
                return false;
            }

            const llama_pos position = static_cast<llama_pos>(start + i);
            if (!llama_batch_ext_set_pos(session->batch, batch_index, &position)) {
                return false;
            }

            const bool last_prompt_token = (start + i + 1 == total);
            if (!llama_batch_ext_set_output_logits(
                    session->batch, batch_index, last_prompt_token)) {
                return false;
            }
        }

        if (llama_process(
                session->context,
                LLAMA_PROCESS_TYPE_DECODE,
                session->batch) != 0) {
            return false;
        }
    }
    return true;
}

bool decode_next_token(SessionHandle *session, llama_token token, llama_pos position) {
    llama_batch_ext_clear(session->batch);
    const int32_t index = llama_batch_ext_add_token(session->batch, 0, token);
    if (index < 0) {
        return false;
    }
    if (!llama_batch_ext_set_pos(session->batch, index, &position)) {
        return false;
    }
    if (!llama_batch_ext_set_output_logits(session->batch, index, true)) {
        return false;
    }
    return llama_process(
        session->context,
        LLAMA_PROCESS_TYPE_DECODE,
        session->batch) == 0;
}

bool append_token_piece(
    JNIEnv *env,
    SessionHandle *session,
    llama_token token,
    jobject sink,
    jmethodID on_token,
    std::string &pending) {
    std::vector<char> buffer(TOKEN_PIECE_BUFFER_BYTES);
    int32_t count = llama_token_to_piece(
        session->vocab, token, buffer.data(),
        static_cast<int32_t>(buffer.size()), 0, true);

    if (count < 0) {
        buffer.resize(static_cast<size_t>(-count));
        count = llama_token_to_piece(
            session->vocab, token, buffer.data(),
            static_cast<int32_t>(buffer.size()), 0, true);
    }
    if (count < 0) {
        return false;
    }

    pending.append(buffer.data(), static_cast<size_t>(count));
    return emit_utf8(env, sink, on_token, pending, false);
}

} // namespace

extern "C" JNIEXPORT void JNICALL
Java_ai_lola_ace_inference_LlamaCppReferenceBackend_nativeInit(
    JNIEnv * /*env*/, jobject /*self*/) {
    std::call_once(g_backend_init_once, [] {
        llama_backend_init();
        g_backend_initialized.store(true);
        __android_log_print(ANDROID_LOG_INFO, LOG_TAG,
                            "llama.cpp backend initialized");
    });
}

extern "C" JNIEXPORT jlong JNICALL
Java_ai_lola_ace_inference_LlamaCppReferenceBackend_nativeLoadModel(
    JNIEnv *env, jobject /*self*/, jstring model_path) {
    if (!g_backend_initialized.load()) {
        throw_java(env, "java/lang/IllegalStateException",
                   "Inference backend was not initialized");
        return 0;
    }
    if (model_path == nullptr) {
        throw_java(env, "java/lang/IllegalArgumentException",
                   "Model path cannot be null");
        return 0;
    }

    const std::string path = jstring_to_utf8(env, model_path);
    if (path.empty()) {
        throw_java(env, "java/lang/IllegalArgumentException", "Model path is empty");
        return 0;
    }

    llama_model_params params = llama_model_default_params();
    params.n_gpu_layers = 0; // CPU reference baseline; no GPU/NPU assumption.

    llama_model *model = llama_model_load_from_file(path.c_str(), params);

    if (model == nullptr) {
        throw_java(env, "java/lang/IllegalStateException",
                   "llama.cpp could not load the GGUF model");
        return 0;
    }

    auto *handle = new ModelHandle();
    handle->model = model;
    try {
        handle->chat_templates = common_chat_templates_init(model, "");
    } catch (const std::exception &error) {
        llama_model_free(model);
        delete handle;
        throw_java(env, "java/lang/IllegalStateException",
                   std::string("Could not initialize the GGUF chat template: ") + error.what());
        return 0;
    }
    if (!handle->chat_templates) {
        llama_model_free(model);
        delete handle;
        throw_java(env, "java/lang/IllegalStateException",
                   "llama.cpp could not initialize a chat template for this GGUF");
        return 0;
    }
    __android_log_print(ANDROID_LOG_INFO, LOG_TAG,
                        "Model and chat template loaded into the CPU reference backend");
    return to_handle(handle);
}

extern "C" JNIEXPORT jlong JNICALL
Java_ai_lola_ace_inference_LlamaCppReferenceBackend_nativeCreateSession(
    JNIEnv *env,
    jobject /*self*/,
    jlong model_handle,
    jint context_tokens,
    jint threads,
    jfloat temperature,
    jint top_k,
    jfloat repeat_penalty,
    jlong seed) {
    auto *model = from_handle<ModelHandle>(model_handle);
    if (model == nullptr || model->model == nullptr) {
        throw_java(env, "java/lang/IllegalArgumentException", "Invalid model handle");
        return 0;
    }
    if (context_tokens < 512 || context_tokens > static_cast<jint>(MAX_CONTEXT_TOKENS)) {
        throw_java(env, "java/lang/IllegalArgumentException",
                   "Reference context must be between 512 and 8192 tokens");
        return 0;
    }
    if (threads < 1 || threads > 8 || top_k < 1 ||
        !(temperature > 0.0f && temperature <= 2.0f) ||
        !(repeat_penalty >= 1.0f && repeat_penalty <= 2.0f)) {
        throw_java(env, "java/lang/IllegalArgumentException",
                   "Invalid reference inference configuration");
        return 0;
    }

    auto *session = new SessionHandle();
    session->owner = model;
    session->context_tokens = static_cast<uint32_t>(context_tokens);

    llama_context_params context_params = llama_context_default_params();
    context_params.n_ctx = session->context_tokens;
    context_params.n_batch = std::min<uint32_t>(session->context_tokens, PROMPT_BATCH_TOKENS);
    context_params.n_ubatch = context_params.n_batch;
    context_params.n_seq_max = 1;
    context_params.n_threads = threads;
    context_params.n_threads_batch = threads;

    session->context = llama_init_from_model(model->model, context_params);
    if (session->context == nullptr) {
        delete session;
        throw_java(env, "java/lang/IllegalStateException",
                   "llama.cpp could not allocate an inference context");
        return 0;
    }

    session->batch = llama_batch_ext_init(session->context);
    session->vocab = llama_model_get_vocab(model->model);

    common_params_sampling sampler_params;
    sampler_params.seed = static_cast<uint32_t>(seed);
    sampler_params.top_k = top_k;
    sampler_params.temp = temperature;
    sampler_params.penalty_last_n = 64;
    sampler_params.penalty_repeat = repeat_penalty;
    sampler_params.penalty_freq = 0.0f;
    sampler_params.penalty_present = 0.0f;
    session->sampler = common_sampler_init(model->model, sampler_params);

    if (session->batch == nullptr || session->vocab == nullptr ||
        session->sampler == nullptr) {
        if (session->sampler != nullptr) common_sampler_free(session->sampler);
        if (session->batch != nullptr) llama_batch_ext_free(session->batch);
        llama_free(session->context);
        delete session;
        throw_java(env, "java/lang/IllegalStateException",
                   "llama.cpp could not initialize session resources");
        return 0;
    }

    return to_handle(session);
}

extern "C" JNIEXPORT jint JNICALL
Java_ai_lola_ace_inference_LlamaCppReferenceBackend_nativeGenerate(
    JNIEnv *env,
    jobject /*self*/,
    jlong session_handle,
    jobjectArray roles,
    jobjectArray contents,
    jint max_output_tokens,
    jobject sink) {
    auto *session = from_handle<SessionHandle>(session_handle);
    if (session == nullptr || roles == nullptr || contents == nullptr || sink == nullptr) {
        throw_java(env, "java/lang/IllegalArgumentException", "Invalid generation arguments");
        return -1;
    }
    if (max_output_tokens < 1 || max_output_tokens > 1024) {
        throw_java(env, "java/lang/IllegalArgumentException",
                   "Output token budget must be between 1 and 1024");
        return -1;
    }
    if (env->GetArrayLength(roles) != env->GetArrayLength(contents)) {
        throw_java(env, "java/lang/IllegalArgumentException",
                   "Chat roles and contents must have the same length");
        return -1;
    }

    std::unique_lock<std::mutex> generation_lock(session->generation_mutex, std::try_to_lock);
    if (!generation_lock.owns_lock()) {
        throw_java(env, "java/lang/IllegalStateException",
                   "This inference session is already generating");
        return -1;
    }

    jclass sink_class = env->GetObjectClass(sink);
    jmethodID on_token = env->GetMethodID(sink_class, "onToken", "(Ljava/lang/String;)V");
    jmethodID on_metrics = env->GetMethodID(sink_class, "onMetrics", "(IIJJJJ)V");
    jmethodID is_cancelled = env->GetMethodID(sink_class, "isCancelled", "()Z");
    env->DeleteLocalRef(sink_class);
    if (on_token == nullptr || on_metrics == nullptr || is_cancelled == nullptr) {
        return -1;
    }

    const auto generation_start = SteadyClock::now();
    const jsize message_count = env->GetArrayLength(roles);
    std::vector<std::string> role_strings;
    std::vector<std::string> content_strings;
    role_strings.reserve(static_cast<size_t>(message_count));
    content_strings.reserve(static_cast<size_t>(message_count));

    for (jsize i = 0; i < message_count; ++i) {
        auto role = static_cast<jstring>(env->GetObjectArrayElement(roles, i));
        auto content = static_cast<jstring>(env->GetObjectArrayElement(contents, i));
        if (role == nullptr || content == nullptr) {
            if (role != nullptr) env->DeleteLocalRef(role);
            if (content != nullptr) env->DeleteLocalRef(content);
            throw_java(env, "java/lang/IllegalArgumentException",
                       "Chat message role and content cannot be null");
            return -1;
        }

        role_strings.emplace_back(jstring_to_utf8(env, role));
        content_strings.emplace_back(jstring_to_utf8(env, content));
        env->DeleteLocalRef(role);
        env->DeleteLocalRef(content);
    }

    std::vector<common_chat_msg> chat;
    chat.reserve(role_strings.size());
    for (size_t i = 0; i < role_strings.size(); ++i) {
        common_chat_msg message;
        message.role = role_strings[i];
        message.content = content_strings[i];
        chat.push_back(std::move(message));
    }

    std::string prompt_text;
    try {
        common_chat_templates_inputs template_inputs;
        template_inputs.messages = std::move(chat);
        template_inputs.use_jinja = true;
        template_inputs.add_generation_prompt = true;
        template_inputs.add_bos = llama_vocab_get_add_bos(session->vocab);
        template_inputs.add_eos = llama_vocab_get_add_eos(session->vocab);
        const common_chat_params formatted = common_chat_templates_apply(
            session->owner->chat_templates.get(), template_inputs);
        prompt_text = formatted.prompt.str();
    } catch (const std::exception &error) {
        throw_java(env, "java/lang/IllegalStateException",
                   std::string("Could not render the LFM3 chat template: ") + error.what());
        return -1;
    }

    if (prompt_text.empty() ||
        prompt_text.size() > static_cast<size_t>(INT32_MAX)) {
        throw_java(env, "java/lang/IllegalArgumentException",
                   "Rendered conversation is empty or too large");
        return -1;
    }

    const int32_t prompt_size = static_cast<int32_t>(prompt_text.size());
    const int32_t token_count_result = llama_tokenize(
        session->vocab, prompt_text.data(), prompt_size,
        nullptr, 0, false, true);
    if (token_count_result == INT32_MIN) {
        throw_java(env, "java/lang/IllegalArgumentException",
                   "Conversation exceeds the tokenizer's supported size");
        return -1;
    }
    const int32_t prompt_tokens_count = token_count_result < 0
        ? -token_count_result : token_count_result;
    if (prompt_tokens_count <= 0 ||
        static_cast<uint32_t>(prompt_tokens_count) + static_cast<uint32_t>(max_output_tokens) >
            session->context_tokens) {
        throw_java(env, "java/lang/IllegalArgumentException",
                   "Conversation exceeds this session's context budget");
        return -1;
    }

    std::vector<llama_token> prompt_tokens(static_cast<size_t>(prompt_tokens_count));
    const int32_t tokenized = llama_tokenize(
        session->vocab, prompt_text.data(), prompt_size,
        prompt_tokens.data(), prompt_tokens_count, false, true);
    if (tokenized != prompt_tokens_count) {
        throw_java(env, "java/lang/IllegalStateException",
                   "llama.cpp failed to tokenize the formatted conversation");
        return -1;
    }

    if (session->cancelled.load()) {
        const auto cancelled_at = SteadyClock::now();
        emit_metrics(
            env, sink, on_metrics, prompt_tokens_count, 0, 0, 0, 0,
            elapsed_millis(generation_start, cancelled_at));
        return 1;
    }
    llama_memory_clear(llama_get_memory(session->context), false);
    common_sampler_reset(session->sampler);

    const auto prefill_start = SteadyClock::now();
    if (!decode_prompt(session, prompt_tokens)) {
        if (session->cancelled.load()) {
            const auto prefill_end = SteadyClock::now();
            emit_metrics(
                env, sink, on_metrics, prompt_tokens_count, 0,
                elapsed_millis(prefill_start, prefill_end), 0, 0,
                elapsed_millis(generation_start, prefill_end));
            return 1;
        }
        throw_java(env, "java/lang/IllegalStateException",
                   "llama.cpp failed while processing the prompt");
        return -1;
    }
    const auto prefill_end = SteadyClock::now();

    std::string pending_utf8;
    llama_pos position = static_cast<llama_pos>(prompt_tokens.size());
    int32_t generated = 0;
    SteadyClock::time_point first_token_time{};

    while (generated < max_output_tokens) {
        if (session->cancelled.load() || callback_cancelled(env, sink, is_cancelled)) {
            session->cancelled.store(true);
            emit_utf8(env, sink, on_token, pending_utf8, true);
            const auto generation_end = SteadyClock::now();
            emit_metrics(
                env, sink, on_metrics, prompt_tokens_count, generated,
                elapsed_millis(prefill_start, prefill_end),
                first_token_time == SteadyClock::time_point{}
                    ? 0
                    : elapsed_millis(generation_start, first_token_time),
                elapsed_millis(prefill_end, generation_end),
                elapsed_millis(generation_start, generation_end));
            return 1;
        }

        const llama_token token = common_sampler_sample(session->sampler, session->context, -1);
        if (token == LLAMA_TOKEN_NULL) {
            throw_java(env, "java/lang/IllegalStateException",
                       "llama.cpp did not select a valid token");
            return -1;
        }
        common_sampler_accept(session->sampler, token, true);
        if (llama_vocab_is_eog(session->vocab, token)) {
            break;
        }
        if (!append_token_piece(env, session, token, sink, on_token, pending_utf8)) {
            if (env->ExceptionCheck()) return -1;
            throw_java(env, "java/lang/IllegalStateException",
                       "llama.cpp could not decode a generated token");
            return -1;
        }

        if (generated == 0) {
            first_token_time = SteadyClock::now();
        }
        ++generated;
        if (generated < max_output_tokens) {
            if (session->cancelled.load()) {
                emit_utf8(env, sink, on_token, pending_utf8, true);
                return 1;
            }
            if (!decode_next_token(session, token, position)) {
                throw_java(env, "java/lang/IllegalStateException",
                           "llama.cpp failed while decoding the next token");
                return -1;
            }
            ++position;
        }
    }

    if (!emit_utf8(env, sink, on_token, pending_utf8, true)) {
        return -1;
    }
    const auto generation_end = SteadyClock::now();
    emit_metrics(
        env, sink, on_metrics, prompt_tokens_count, generated,
        elapsed_millis(prefill_start, prefill_end),
        first_token_time == SteadyClock::time_point{}
            ? 0
            : elapsed_millis(generation_start, first_token_time),
        elapsed_millis(prefill_end, generation_end),
        elapsed_millis(generation_start, generation_end));
    return 0;
}

extern "C" JNIEXPORT void JNICALL
Java_ai_lola_ace_inference_LlamaCppReferenceBackend_nativeResetCancellation(
    JNIEnv * /*env*/, jobject /*self*/, jlong session_handle) {
    auto *session = from_handle<SessionHandle>(session_handle);
    if (session != nullptr) {
        session->cancelled.store(false);
    }
}

extern "C" JNIEXPORT void JNICALL
Java_ai_lola_ace_inference_LlamaCppReferenceBackend_nativeCancel(
    JNIEnv * /*env*/, jobject /*self*/, jlong session_handle) {
    auto *session = from_handle<SessionHandle>(session_handle);
    if (session != nullptr) {
        session->cancelled.store(true);
    }
}

extern "C" JNIEXPORT void JNICALL
Java_ai_lola_ace_inference_LlamaCppReferenceBackend_nativeCloseSession(
    JNIEnv * /*env*/, jobject /*self*/, jlong session_handle) {
    auto *session = from_handle<SessionHandle>(session_handle);
    if (session == nullptr) return;

    session->cancelled.store(true);
    if (session->sampler != nullptr) common_sampler_free(session->sampler);
    if (session->batch != nullptr) llama_batch_ext_free(session->batch);
    if (session->context != nullptr) llama_free(session->context);
    delete session;
}

extern "C" JNIEXPORT void JNICALL
Java_ai_lola_ace_inference_LlamaCppReferenceBackend_nativeUnloadModel(
    JNIEnv * /*env*/, jobject /*self*/, jlong model_handle) {
    auto *model = from_handle<ModelHandle>(model_handle);
    if (model == nullptr) return;
    model->chat_templates.reset();
    if (model->model != nullptr) llama_model_free(model->model);
    delete model;
}

extern "C" JNIEXPORT void JNICALL
Java_ai_lola_ace_inference_LlamaCppReferenceBackend_nativeShutdown(
    JNIEnv * /*env*/, jobject /*self*/) {
    if (g_backend_initialized.exchange(false)) {
        llama_backend_free();
    }
}
