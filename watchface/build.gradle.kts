import java.util.Properties
plugins { id("com.android.application") }

val keyProps = Properties().apply {
    val f = file(System.getenv("CM_KEYSTORE_PROPS") ?: (System.getProperty("user.home") + "/.android/hearth-release.properties"))
    if (f.exists()) f.inputStream().use { load(it) }
}

val v = (file("../wear/build.gradle.kts").readText())
android {
    namespace = "dev.voer.hearth.face"
    compileSdk = 35
    defaultConfig {
        applicationId = "dev.voer.hearth.face"
        minSdk = 33
        targetSdk = 35
        versionCode = Regex("versionCode\\s*=\\s*(\\d+)").find(v)!!.groupValues[1].toInt()
        versionName = Regex("versionName\\s*=\\s*\"([^\"]+)\"").find(v)!!.groupValues[1]
    }
    signingConfigs {
        if (!keyProps.isEmpty) create("release") {
            storeFile = file(keyProps.getProperty("storeFile")); storePassword = keyProps.getProperty("storePassword")
            keyAlias = keyProps.getProperty("keyAlias"); keyPassword = keyProps.getProperty("keyPassword")
        }
    }
    buildTypes {
        release {
            isMinifyEnabled = false
            isShrinkResources = false   // a watch face is only resources: shrinking can remove what the XML names
            signingConfig = signingConfigs.getByName(if (keyProps.isEmpty) "debug" else "release")
        }
    }
}
