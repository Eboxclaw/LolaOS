package ai.lola.ace

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import ai.lola.ace.ui.LolaAceApp
import ai.lola.ace.ui.theme.LolaAceTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            LolaAceTheme {
                LolaAceApp()
            }
        }
    }
}
