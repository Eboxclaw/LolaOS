plugins {
    id("com.android.application")
}

android {
    namespace = "tools.lola.adbime"
    compileSdk = 36

    defaultConfig {
        applicationId = "tools.lola.adbime"
        minSdk = 28
        targetSdk = 36
        versionCode = 1
        versionName = "1.0"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }
}
