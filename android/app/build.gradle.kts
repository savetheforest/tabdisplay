plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "com.tabdisplay"
    compileSdk = 36
    defaultConfig {
        applicationId = "com.tabdisplay"
        minSdk = 30
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
    }
    buildTypes {
        release {
            // ponytail: signed with this machine's debug key so the installer can sideload it;
            // a real keystore is needed for the Play Store or installs across machines.
            signingConfig = signingConfigs.getByName("debug")
            // Compose pulls in whole libraries; R8 trims the APK from ~20 MB to a few MB.
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"))
        }
    }
    buildFeatures {
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
}
