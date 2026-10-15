# SPDX-License-Identifier: MPL-2.0
#
# R8 rules for the release build. The JNA and bindings keep rules come from
# :core-bindings (consumer-rules.pro).

# Nothing reaches logcat from a release build (threat model R24): calls to
# android.util.Log are removed. The methods are listed because a `{ *; }`
# rule would also match java.lang.Object's methods (R8 warns about that).
-assumenosideeffects class android.util.Log {
    public static int v(...);
    public static int d(...);
    public static int i(...);
    public static int w(...);
    public static int e(...);
    public static int wtf(...);
    public static int println(...);
    public static java.lang.String getStackTraceString(...);
}
