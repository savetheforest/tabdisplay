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
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}
