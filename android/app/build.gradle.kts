plugins {
    id("com.android.application")
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
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}
