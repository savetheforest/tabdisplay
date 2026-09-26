import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

// Release signing key: environment variables (CI) or ~/.tabdisplay/android-release.properties (see README).
// Never in the repo. Without it the release build falls back to this machine's debug key.
val releaseProps = Properties().apply {
    val file = File(System.getProperty("user.home"), ".tabdisplay/android-release.properties")
    if (file.exists()) file.inputStream().use { load(it) }
}
fun releaseKey(env: String, prop: String): String? = System.getenv(env) ?: releaseProps.getProperty(prop)
val releaseStore = releaseKey("TABDISPLAY_KEYSTORE", "storeFile")

android {
    namespace = "com.tabdisplay"
    compileSdk = 36
    defaultConfig {
        applicationId = "com.tabdisplay"
        minSdk = 30
        targetSdk = 36
        versionCode = 2
        versionName = "0.2.0"
        // Crash reporting stays off without a DSN: TABDISPLAY_SENTRY_DSN=... ./gradlew assembleRelease
        buildConfigField("String", "SENTRY_DSN", "\"${System.getenv("TABDISPLAY_SENTRY_DSN") ?: ""}\"")
    }
    signingConfigs {
        if (releaseStore != null) {
            create("release") {
                storeFile = file(releaseStore)
                storePassword = releaseKey("TABDISPLAY_KEYSTORE_PASSWORD", "storePassword")
                keyAlias = releaseKey("TABDISPLAY_KEY_ALIAS", "keyAlias")
                keyPassword = releaseKey("TABDISPLAY_KEY_PASSWORD", "keyPassword")
            }
        }
    }
    buildTypes {
        release {
            signingConfig = if (releaseStore != null) {
                signingConfigs.getByName("release")
            } else {
                logger.warn("No release keystore configured: signing with the debug key (fine for local tests, not for distribution).")
                signingConfigs.getByName("debug")
            }
            // Compose pulls in whole libraries; R8 trims the APK from ~20 MB to a few MB.
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"))
        }
    }
    buildFeatures {
        buildConfig = true
        compose = true
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.compose.material3:material3:1.4.0")
    implementation("io.sentry:sentry-android:8.20.0")
}
