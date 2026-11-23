pluginManagement {
    includeBuild("build-logic")
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode = RepositoriesMode.FAIL_ON_PROJECT_REPOS
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "RommSaveSyncer"

include(
    ":core:model",
    ":core:romm",
    ":core:db",
    ":core:saves",
    ":core:matching",
    ":core:engine",
    ":core:ipc",
    ":ui:shared",
    ":desktop:app",
    ":desktop:platform",
    ":desktop:hooks",
    ":android:app",
    ":testing:romm-harness",
    ":testing:fixtures",
)
