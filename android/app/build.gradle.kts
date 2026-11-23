plugins {
    id("rommsavesyncer.android-app")
    id("rommsavesyncer.compose")
}

dependencies {
    implementation(project(":ui:shared"))
    implementation(libs.androidx.activity.compose)
    implementation(libs.compose.runtime)
}
