---
name: build-gotchas
description: Build/toolchain gotchas found in M1 (Gradle 9.9.2, AGP 9.5.2, Kotlin 2.4.21, Compose MP 1.12.2, sigil JNI on MSVC)
metadata:
  type: project
---

- Compose Desktop task for the Linux app folder is `createReleaseDistributable` (plan's `packageReleaseDistributable` does not exist). MSI: `packageReleaseMsi`; needs WiX 3 (not on the dev PC; CI windows-latest has it).
- androidx.compose 1.12 pulled in by CMP 1.12.2 requires compileSdk 37; AGP auto-downloads the platform (licenses accepted locally). compileSdk/targetSdk 37, minSdk 33.
- `org.jetbrains.compose.material3` has no stable 1.12 release (only 1.9.0 stable / 1.13 alphas): M1 UI uses foundation `BasicText`. Decide the material3 version when needed.
- AGP 9 KMP library: `com.android.kotlin.multiplatform.library`; configure via `(kotlin as ExtensionAware).extensions.getByType<KotlinMultiplatformAndroidLibraryTarget>()` (no `androidLibrary {}` accessor). Android app has built-in Kotlin (no kotlin.android plugin). commonTest dir triggers a warning unless android host tests enabled; M1 puts tests in jvmTest.
- detekt 2.0.0-alpha.6 (`dev.detekt`) config rejects a `build:` key. detekt/ktlint only lint `src`; ktlint excludes `/third_party/`.
- sigil v1.0.0 JNI source includes `<unistd.h>`, which MSVC lacks: stub in `native/msvc-compat/unistd.h` passed via `CFLAGS` env on a fresh CMake cache (task always wipes its cmake dir). sigil needs `git submodule update --init --recursive` (libchdr, tiny-AES-c). Never `submodule update` before `git add`-ing the tag pin (it resets to the recorded commit).
- Heredoc/Write files must end with a newline or ktlint fails; run `./gradlew ktlintFormat`.
- The auto-mode classifier blocks Writer from running `gradlew test` (validation is the Tester's job); compile/lint/assemble tasks are allowed.
