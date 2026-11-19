package ai.lola.ace.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

private val LolaColors = darkColorScheme(
    primary = Color(0xFFB7C7FF),
    onPrimary = Color(0xFF17264F),
    secondary = Color(0xFFB8DCCB),
    onSecondary = Color(0xFF17372B),
    background = Color(0xFF101319),
    onBackground = Color(0xFFE1E5EF),
    surface = Color(0xFF171C24),
    onSurface = Color(0xFFE1E5EF),
    surfaceVariant = Color(0xFF272E39),
    onSurfaceVariant = Color(0xFFC2C9D6),
)

@Composable
fun LolaAceTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = LolaColors,
        content = content,
    )
}
