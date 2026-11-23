/*
 * Stand-in for the POSIX <unistd.h> that MSVC lacks.
 *
 * sigil v1.0.0's JNI source (bindings/android/src/main/cpp/sigil_jni.c) includes <unistd.h>
 * but needs nothing from it on Windows. We cannot edit the sigil submodule, so the Gradle
 * native build puts this folder on the include path for MSVC builds only.
 */
#ifndef ROMMSAVESYNCER_MSVC_UNISTD_H
#define ROMMSAVESYNCER_MSVC_UNISTD_H

#include <io.h>
#include <process.h>

#endif
