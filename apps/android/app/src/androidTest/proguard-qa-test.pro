# SPDX-License-Identifier: MPL-2.0
#
# R8 rules for the qa build's test APK (minifyQaAndroidTestWithR8, which
# AGP runs with the app's mapping). Tests are found by name through runner
# arguments and reflection, so nothing in the test APK is removed, renamed
# or optimized; its references to the app follow the app's mapping.
-dontshrink
-dontoptimize
-dontobfuscate

# Annotation-only references in the Accessibility Test Framework and Error
# Prone whose classes are not on the classpath; never loaded at run time.
-dontwarn com.google.auto.value.AutoValue
-dontwarn com.google.auto.value.AutoValue$Builder
-dontwarn javax.lang.model.element.Modifier
