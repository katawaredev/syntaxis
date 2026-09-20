plugins { id("com.android.application") }

val appVersion = Regex("""(?m)^version = "([^"]+)"$""")
    .find(rootProject.file("../server/Cargo.toml").readText())!!.groupValues[1]
val versionParts = appVersion.split('.').map { it.toInt() }
require(versionParts.size == 3 && versionParts[1] < 1000 && versionParts[2] < 1000)

android {
    namespace = "dev.syntaxis.android"
    compileSdk { version = release(36) { minorApiLevel = 1 } }
    defaultConfig {
        applicationId = "dev.syntaxis.android"
        minSdk = 26
        targetSdk = 36
        versionCode = versionParts[0] * 1000000 + versionParts[1] * 1000 + versionParts[2]
        versionName = appVersion
    }
    val signingPath = System.getenv("ANDROID_KEYSTORE_PATH")
    if (!signingPath.isNullOrBlank()) {
        signingConfigs {
            create("release") {
                storeFile = file(signingPath)
                storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("ANDROID_KEY_ALIAS")
                keyPassword = System.getenv("ANDROID_KEY_PASSWORD")
            }
        }
        buildTypes.getByName("release").signingConfig = signingConfigs.getByName("release")
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation("androidx.webkit:webkit:1.12.1")
    testImplementation("junit:junit:4.13.2")
}
