import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

// The watch app is signed with the same key as the phone app, so the two can talk over the Wear OS data layer.
val keyProps = Properties().apply {
    val f = file(System.getenv("CM_KEYSTORE_PROPS") ?: (System.getProperty("user.home") + "/.android/hearth-release.properties"))
    if (f.exists()) f.inputStream().use { load(it) }
}

android {
    namespace = "dev.voer.hearth.wear"
    compileSdk = 35
    defaultConfig {
        applicationId = "dev.voer.hearth"
        minSdk = 30
        targetSdk = 35
        versionCode = 13401
        versionName = "1.34.1"
    }
    signingConfigs {
        if (!keyProps.isEmpty) create("release") {
            storeFile = file(keyProps.getProperty("storeFile")); storePassword = keyProps.getProperty("storePassword")
            keyAlias = keyProps.getProperty("keyAlias"); keyPassword = keyProps.getProperty("keyPassword")
        }
    }
    buildTypes {
        debug { applicationIdSuffix = ".watchtest" }   // a test build never replaces the real phone app when it is tried on a phone
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName(if (keyProps.isEmpty) "debug" else "release")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlin { compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) } }
    buildFeatures { compose = true }
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2025.09.00"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.wear.compose:compose-material:1.4.1")
    implementation("androidx.wear.compose:compose-foundation:1.4.1")
    implementation("androidx.activity:activity-compose:1.10.1")
    implementation("androidx.core:core-ktx:1.16.0")
    implementation("com.google.android.gms:play-services-wearable:19.0.0")
    implementation("androidx.fragment:fragment-ktx:1.8.9")   // play services brings an old Fragment that release lint refuses
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-play-services:1.10.2")
}
