# RommSaveSyncer

Keeps emulator saves in sync between your devices through a [RomM](https://github.com/rommapp/romm) server. It runs
alongside your existing launcher (ES-DE, Steam, ...) on Windows, Linux (Bazzite) and Android. See
[GLOSSARY.md](GLOSSARY.md) for the terms, [docs/PLAN.md](docs/PLAN.md) for the plan and milestones, and
[docs/adr](docs/adr) for design decisions.

Status: milestone M1 (scaffolding). The app only shows a hello window and a hello activity so far.

License: GPL-3.0 (see [LICENSE](LICENSE)).

## Build and test

Requirements: JDK 21 (Temurin), the Android SDK (`local.properties` with `sdk.dir=...` or `ANDROID_HOME`), and the
sigil submodule:

```sh
git submodule update --init --recursive
```

sigil's JNI library is built by Gradle with CMake: MSVC (Visual Studio with the C++ tools) on Windows, gcc on Linux.
Gradle finds CMake through `-Pcmake.path=...`, the `CMAKE` environment variable, the CMake bundled with Visual Studio
(Windows), or `cmake` on the PATH, in that order.

```sh
./gradlew check                                    # unit tests, ktlint, detekt, Android lint
./gradlew :desktop:app:run                         # hello window
./gradlew :desktop:app:createReleaseDistributable  # app folder with bundled JRE (tar it for Linux)
./gradlew :desktop:app:packageReleaseMsi           # Windows installer (Compose Gradle plugin downloads WiX itself)
./gradlew :android:app:assembleDebug               # debug APK
./gradlew ktlintFormat                             # fix formatting
```

CI (`.github/workflows/ci.yml`) runs `check` on Linux and Windows and publishes the MSI, the Linux tarball and the
debug APK as artifacts.
