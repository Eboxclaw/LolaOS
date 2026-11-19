package ai.lola.ace.inference

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class InferenceRunMetricsTest {
    @Test
    fun calculatesDecodeTokensPerSecondFromNativeDecodeWindow() {
        val metrics = InferenceRunMetrics(
            promptTokens = 120,
            generatedTokens = 40,
            prefillMillis = 1_500,
            timeToFirstTokenMillis = 1_620,
            decodeMillis = 2_000,
            totalMillis = 3_620,
        )

        assertEquals(20.0, metrics.decodeTokensPerSecond!!, 0.0001)
    }

    @Test
    fun returnsNoThroughputWhenNoTokensWereGenerated() {
        val metrics = InferenceRunMetrics(
            promptTokens = 120,
            generatedTokens = 0,
            prefillMillis = 1_500,
            timeToFirstTokenMillis = 0,
            decodeMillis = 800,
            totalMillis = 2_300,
        )

        assertNull(metrics.decodeTokensPerSecond)
    }

    @Test
    fun returnsNoThroughputWhenDecodeWindowIsZero() {
        val metrics = InferenceRunMetrics(
            promptTokens = 120,
            generatedTokens = 20,
            prefillMillis = 1_500,
            timeToFirstTokenMillis = 1_600,
            decodeMillis = 0,
            totalMillis = 1_600,
        )

        assertNull(metrics.decodeTokensPerSecond)
    }

    @Test
    fun rejectsNegativeMeasurements() {
        try {
            InferenceRunMetrics(
                promptTokens = 1,
                generatedTokens = 1,
                prefillMillis = -1,
                timeToFirstTokenMillis = 0,
                decodeMillis = 1,
                totalMillis = 2,
            )
            throw AssertionError("Expected an invalid metrics sample to be rejected")
        } catch (_: IllegalArgumentException) {
            // Expected.
        }
    }
}
