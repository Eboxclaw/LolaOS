plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.compose.compiler)
}

android {
    namespace = "ai.lola.ace"
    compileSdk = 36
    ndkVersion = "29.0.13113456"

    // Monotonic version scheme for over-the-air updates on test devices:
    // CI passes GITHUB_RUN_NUMBER, so every main-branch build has a strictly
    // higher versionCode than the previous one and `adb install -r` (or
    // package-installer updates) always succeeds with the same signature.
    // Local builds default to 1 and must not be sideloaded over CI builds
    // (or pass -PversionCode=<n> higher than the installed build).
    val ciRunNumber = System.getenv("GITHUB_RUN_NUMBER")?.toIntOrNull()
    val versionCodeValue = (project.findProperty("versionCode") as String?)?.toIntOrNull()
        ?: ciRunNumber
        ?: 1

    defaultConfig {
        applicationId = "ai.lola.ace"
        minSdk = 28
        targetSdk = 36
        versionCode = versionCodeValue
        versionName = "0.1.${versionCodeValue}"

        ndk {
            abiFilters += listOf("arm64-v8a")
        }

        externalNativeBuild {
            cmake {
                cppFlags += listOf("-std=c++17")
            }
        }
    }

    signingConfigs {
        create("release") {
            // Dev-testing signing key, committed so any machine and CI
            // produce interoperable (update-compatible) APKs. Replace with
            // a secret-held production key before ANY public distribution.
            storeFile = rootProject.file("keystore/lola-ace-release.jks")
            storePassword = "lola-ace-dev"
            keyAlias = "lola-ace"
            keyPassword = "lola-ace-dev"
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            isShrinkResources = false
            signingConfig = signingConfigs.getByName("release")
        }
    }

    buildFeatures {
        compose = true
    }

    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
            version = "3.31.6"
        }
    }

    sourceSets {
        getByName("main") {
            // Ship the exact catalogue consumed by the Rust ace-models crate.
            assets.srcDir("../../models")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    testOptions {
        unitTests.isIncludeAndroidResources = false
    }
}

dependencies {
    implementation(libs.androidx.activity.compose)
    implementation(libs.kotlinx.coroutines.android)

    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.graphics)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.compose.material3)

    debugImplementation(libs.androidx.compose.ui.tooling)
    testImplementation(libs.junit)
}
