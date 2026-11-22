package ai.lola.ace.ui.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * LOLA ACE design tokens.
 *
 * Anchors (unchanged from the first build): periwinkle primary, mint
 * secondary, ink background. Everything else is a tonal derivation so the
 * three existing anchors remain the recognizable core. No external fonts,
 * no icon packs, no animation libraries — zero dependencies by rule.
 */

private val LolaColors = darkColorScheme(
    primary = Color(0xFFB7C7FF),
    onPrimary = Color(0xFF17264F),
    primaryContainer = Color(0xFF445079),
    onPrimaryContainer = Color(0xFFDBE2FF),
    secondary = Color(0xFFB8DCCB),
    onSecondary = Color(0xFF17372B),
    secondaryContainer = Color(0xFF2C463A),
    onSecondaryContainer = Color(0xFFCDEBDD),
    tertiary = Color(0xFFD6CEFF),
    onTertiary = Color(0xFF261E4E),
    background = Color(0xFF101319),
    onBackground = Color(0xFFE1E5EF),
    surface = Color(0xFF171C24),
    onSurface = Color(0xFFE1E5EF),
    surfaceVariant = Color(0xFF272E39),
    onSurfaceVariant = Color(0xFFC2C9D6),
    outline = Color(0xFF8A93A6),
    outlineVariant = Color(0xFF3A424F),
)

/** Status roles outside the Material scheme: verification, degraded, informational. */
object LolaStatus {
    val Success = Color(0xFF9AD6A8)
    val onSuccess = Color(0xFF143324)
    val Warning = Color(0xFFF2CD6B)
    val onWarning = Color(0xFF3B2F07)
    val Info = Color(0xFFA9C7FF)
    val onInfo = Color(0xFF16233F)
}

/** One spacing rhythm for every screen. */
object LolaSpacing {
    val xs = 4.dp
    val sm = 8.dp
    val md = 12.dp
    val lg = 16.dp
    val xl = 24.dp
    val xxl = 32.dp
}

/** One geometry scale: bubbles, cards, sheets and chips share these radii. */
private val LolaShapes = Shapes(
    extraSmall = RoundedCornerShape(8.dp),
    small = RoundedCornerShape(12.dp),
    medium = RoundedCornerShape(16.dp),
    large = RoundedCornerShape(24.dp),
    extraLarge = RoundedCornerShape(28.dp),
)

private val LolaTypography = Typography(
    headlineSmall = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.SemiBold,
        fontSize = 24.sp,
        lineHeight = 30.sp,
        letterSpacing = (-0.25).sp,
    ),
    titleLarge = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Medium,
        fontSize = 20.sp,
        lineHeight = 26.sp,
        letterSpacing = 0.sp,
    ),
    titleMedium = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Medium,
        fontSize = 16.sp,
        lineHeight = 22.sp,
        letterSpacing = 0.1.sp,
    ),
    bodyMedium = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Normal,
        fontSize = 14.sp,
        lineHeight = 21.sp,
        letterSpacing = 0.2.sp,
    ),
    bodySmall = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.Normal,
        fontSize = 12.sp,
        lineHeight = 17.sp,
        letterSpacing = 0.3.sp,
    ),
    labelLarge = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.SemiBold,
        fontSize = 14.sp,
        lineHeight = 18.sp,
        letterSpacing = 0.2.sp,
    ),
    labelSmall = TextStyle(
        fontFamily = FontFamily.SansSerif,
        fontWeight = FontWeight.SemiBold,
        fontSize = 11.sp,
        lineHeight = 14.sp,
        letterSpacing = 0.8.sp,
    ),
)

@Composable
fun LolaAceTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = LolaColors,
        typography = LolaTypography,
        shapes = LolaShapes,
        content = content,
    )
}
