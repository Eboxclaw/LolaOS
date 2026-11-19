package ai.lola.ace.inference

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class LocalInferenceBackendContractTest {
    @Test
    fun acceptsDefaultReferenceSessionConfiguration() {
        val config = ReferenceSessionConfig()

        assertEquals(2048, config.contextTokens)
        assertEquals(4, config.threads)
        assertEquals(0.1f, config.temperature)
        assertEquals(50, config.topK)
        assertEquals(1.1f, config.repeatPenalty)
    }

    @Test
    fun acceptsUpperContextAndThreadBoundaries() {
        val config = ReferenceSessionConfig(
            contextTokens = 8192,
            threads = 8,
            temperature = 2.0f,
            topK = 1,
            repeatPenalty = 2.0f,
        )

        assertEquals(8192, config.contextTokens)
        assertEquals(8, config.threads)
    }

    @Test
    fun rejectsContextOutsideHostLimit() {
        expectIllegalArgument { ReferenceSessionConfig(contextTokens = 511) }
        expectIllegalArgument { ReferenceSessionConfig(contextTokens = 8193) }
    }

    @Test
    fun rejectsThreadCountOutsideHostLimit() {
        expectIllegalArgument { ReferenceSessionConfig(threads = 0) }
        expectIllegalArgument { ReferenceSessionConfig(threads = 9) }
    }

    @Test
    fun rejectsInvalidSamplingParameters() {
        expectIllegalArgument { ReferenceSessionConfig(temperature = 0f) }
        expectIllegalArgument { ReferenceSessionConfig(temperature = Float.NaN) }
        expectIllegalArgument { ReferenceSessionConfig(temperature = Float.POSITIVE_INFINITY) }
        expectIllegalArgument { ReferenceSessionConfig(topK = 0) }
        expectIllegalArgument { ReferenceSessionConfig(repeatPenalty = 0.9f) }
        expectIllegalArgument { ReferenceSessionConfig(repeatPenalty = Float.NaN) }
    }

    @Test
    fun acceptsOnlyExplicitChatRolesAndNonBlankContent() {
        val user = InferenceChatMessage(role = "user", content = "Olá")
        val assistant = InferenceChatMessage(role = "assistant", content = "Olá!")
        assertTrue(user.role == "user")
        assertTrue(assistant.role == "assistant")

        expectIllegalArgument { InferenceChatMessage(role = "tool", content = "output") }
        expectIllegalArgument { InferenceChatMessage(role = "user", content = "  ") }
    }

    @Test
    fun systemAndAssistantRolesAreNotTreatedAsUserMessages() {
        val system = InferenceChatMessage("system", "Instruções")
        val assistant = InferenceChatMessage("assistant", "Resposta")
        assertFalse(system.role == "user")
        assertFalse(assistant.role == "user")
    }

    private inline fun expectIllegalArgument(block: () -> Unit) {
        try {
            block()
            throw AssertionError("Expected IllegalArgumentException")
        } catch (_: IllegalArgumentException) {
            // Expected.
        }
    }
}
