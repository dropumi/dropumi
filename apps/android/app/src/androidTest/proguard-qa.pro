# SPDX-License-Identifier: MPL-2.0
#
# Extra R8 rules for the app APK of the qa build only (the build
# scripts/android/interop.sh runs); release never uses them.
#
# The test APK runs in the app's process, and AGP leaves out of it every
# class the app APK already has (Kotlin, kotlinx.coroutines,
# androidx.tracing), so whatever the interop run calls in the app must
# survive R8 there. These rules keep only the classes and members that the
# code of an interop run in the test APK references: DropumiTestRunner and
# the androidx.test runner, TestApplication's interop graph and
# AndroidInteropTest (TestApplication's fake graph never runs on qa). R8
# shrinks, optimizes and renames everything else as it does for release.
#
# allowobfuscation: AGP runs R8 on the test APK with the app's mapping
# (-applymapping, proguard-qa-test.pro), so a renamed class or member is
# still found, and an override in the test APK gets the new name.
#
# What qa does not show: a kept member is not removed, inlined, merged or
# given another signature in qa, while release may do that to it. So a
# release-only R8 problem in exactly these members cannot appear in an
# interop run (ADR-0013 D10, docs/testing.md). Not kept on purpose: the
# loading of kotlinx.coroutines' Main dispatcher and its service types
# (keeping them would stop the ServiceLoader rewriting release gets), and
# everything the bindings' own rules do not keep.
#
# The list is what the qa test APK's dex references outside itself and the
# framework, from the code an interop run reaches (compared with dexdump
# against the qa app's dex and mapping). Update it with the interop
# sources. A missing rule shows up in an interop run as
# NoClassDefFoundError, NoSuchMethodError, AbstractMethodError or
# VerifyError. The bindings (io.github.dropumi.ffi) and JNA keep their
# release rules (core-bindings/consumer-rules.pro).

# :app
-keep,allowobfuscation class io.github.dropumi.AppGraph {
    io.github.dropumi.appstate.AppLoader getLoader();
}
-keep,allowobfuscation class io.github.dropumi.AppGraphHost {
    io.github.dropumi.AppGraph getGraph();
}
-keep,allowobfuscation class io.github.dropumi.LiveGraph {
    io.github.dropumi.LiveGraph INSTANCE;
    io.github.dropumi.AppGraph build(android.app.Application, io.github.dropumi.core.AppPaths, io.github.dropumi.appstate.SettingsStore);
}
-keep,allowobfuscation class io.github.dropumi.MainActivity
-keep,allowobfuscation class io.github.dropumi.ProcessVisibility {
    io.github.dropumi.ProcessVisibility$Companion Companion;
}
-keep,allowobfuscation class io.github.dropumi.ProcessVisibility$Companion {
    void install(io.github.dropumi.AppGraphHost);
}

# :appstate
-keep,allowobfuscation class io.github.dropumi.appstate.AppLoader {
    kotlinx.coroutines.flow.StateFlow getState();
    void load();
}
-keep,allowobfuscation class io.github.dropumi.appstate.AppLoader$State
-keep,allowobfuscation class io.github.dropumi.appstate.AppLoader$State$Loading
-keep,allowobfuscation class io.github.dropumi.appstate.AppLoader$State$Ready {
    io.github.dropumi.appstate.AppModel getModel();
}
-keep,allowobfuscation class io.github.dropumi.appstate.AppModel {
    kotlinx.coroutines.flow.StateFlow getState();
    void addToDraft(java.util.List);
    java.lang.Object answer(io.github.dropumi.ffi.Prompt, boolean, kotlin.coroutines.Continuation);
    void beginBrowsing();
    void endBrowsing();
    java.lang.Object finishOnboarding(io.github.dropumi.appstate.ReceiveVisibility, kotlin.coroutines.Continuation);
    java.lang.Object pair(io.github.dropumi.ffi.SendTarget, java.lang.String, kotlin.coroutines.Continuation);
    void promptElapsed-E0BElUM(long, int, long);
    java.lang.Object refreshTrusted(kotlin.coroutines.Continuation);
    java.lang.Object sendDraft(io.github.dropumi.ffi.SendTarget, java.lang.String, boolean, kotlin.coroutines.Continuation);
}
-keep,allowobfuscation class io.github.dropumi.appstate.AppModelKt {
    long getPromptId(io.github.dropumi.ffi.Prompt);
}
-keep,allowobfuscation class io.github.dropumi.appstate.AppState {
    io.github.dropumi.core.discovery.AdvertiseState getAdvertising();
    io.github.dropumi.appstate.PromptArming getArming();
    io.github.dropumi.appstate.Banner getBanner();
    io.github.dropumi.ffi.Prompt getCurrentPrompt();
    java.lang.String getDiscoveryProblem();
    java.util.List getDraft();
    java.util.List getHistory();
    io.github.dropumi.ffi.IdentityInfo getIdentity();
    java.util.List getNearby();
    io.github.dropumi.appstate.PairingSession getPairing();
    java.util.List getPrompts();
    io.github.dropumi.ffi.ReceiverInfo getReceiver();
    java.util.List getTrusted();
}
-keep,allowobfuscation class io.github.dropumi.appstate.Banner {
    java.lang.String getTitle();
}
-keep,allowobfuscation class io.github.dropumi.appstate.HistoryEntry {
    io.github.dropumi.appstate.HistoryEntry$Outcome getOutcome();
    java.util.List getSavedAs();
}
-keep,allowobfuscation class io.github.dropumi.appstate.HistoryEntry$Outcome {
    io.github.dropumi.appstate.HistoryEntry$Outcome COMPLETED;
}
-keep,allowobfuscation class io.github.dropumi.appstate.MemorySettingsStore {
    <init>(int);
}
-keep,allowobfuscation class io.github.dropumi.appstate.OutgoingFile {
    <init>(java.lang.String, kotlin.ULong, io.github.dropumi.appstate.OutgoingFile$Source, java.lang.String, int, kotlin.jvm.internal.DefaultConstructorMarker);
    java.lang.String getName();
    io.github.dropumi.appstate.OutgoingFile$Source getSource();
}
-keep,allowobfuscation class io.github.dropumi.appstate.OutgoingFile$Source
-keep,allowobfuscation class io.github.dropumi.appstate.OutgoingFile$Source$Document
-keep,allowobfuscation class io.github.dropumi.appstate.OutgoingFile$Source$Path {
    <init>(java.lang.String, java.lang.String, int, kotlin.jvm.internal.DefaultConstructorMarker);
}
-keep,allowobfuscation class io.github.dropumi.appstate.PairingSession {
    io.github.dropumi.ffi.PairingResult getResult();
}
-keep,allowobfuscation class io.github.dropumi.appstate.PromptArming {
    boolean canAccept-VKZWuLQ(long);
    int getEpoch();
    kotlin.ULong getPromptId-6VbMDqA();
}
-keep,allowobfuscation class io.github.dropumi.appstate.ReceiveVisibility {
    io.github.dropumi.appstate.ReceiveVisibility EVERYONE;
    io.github.dropumi.appstate.ReceiveVisibility PAIRED_ONLY;
}
-keep,allowobfuscation class io.github.dropumi.appstate.ServiceStarter
-keep,allowobfuscation class io.github.dropumi.appstate.SettingsStore

# :core
-keep,allowobfuscation class io.github.dropumi.core.AppPaths {
    io.github.dropumi.core.AppPaths$Companion Companion;
    java.io.File getConfigDir();
    java.io.File getReceiveDir();
}
-keep,allowobfuscation class io.github.dropumi.core.AppPaths$Companion {
    io.github.dropumi.core.AppPaths under(java.io.File);
}
-keep,allowobfuscation class io.github.dropumi.core.discovery.AdvertiseState
-keep,allowobfuscation class io.github.dropumi.core.discovery.AdvertiseState$GateClosed {
    io.github.dropumi.core.discovery.AdvertiseState$GateClosed INSTANCE;
}
-keep,allowobfuscation class io.github.dropumi.core.discovery.AdvertiseState$Registered {
    io.github.dropumi.core.discovery.AdvertiseState$Registered INSTANCE;
}

# The Kotlin standard library. Facade classes (CollectionsKt and the like)
# are kept with the part classes that declare the members the tests call.
-keep,allowobfuscation class kotlin.Lazy {
    java.lang.Object getValue();
}
-keep,allowobfuscation class kotlin.LazyKt
-keep,allowobfuscation class kotlin.LazyKt__LazyJVMKt {
    kotlin.Lazy lazy(kotlin.jvm.functions.Function0);
}
-keep,allowobfuscation class kotlin.Pair
-keep,allowobfuscation class kotlin.ResultKt {
    void throwOnFailure(java.lang.Object);
}
-keep,allowobfuscation class kotlin.TuplesKt {
    kotlin.Pair to(java.lang.Object, java.lang.Object);
}
-keep,allowobfuscation class kotlin.ULong {
    kotlin.ULong box-impl(long);
    long constructor-impl(long);
    long unbox-impl();
}
-keep,allowobfuscation class kotlin.UShort {
    java.lang.String toString-impl(short);
}
-keep,allowobfuscation class kotlin.Unit {
    kotlin.Unit INSTANCE;
}
-keep,allowobfuscation class kotlin.collections.ArraysKt
-keep,allowobfuscation class kotlin.collections.ArraysKt___ArraysKt {
    java.lang.Object firstOrNull(java.lang.Object[]);
    java.lang.String joinToString$default(byte[], java.lang.CharSequence, java.lang.CharSequence, java.lang.CharSequence, int, java.lang.CharSequence, kotlin.jvm.functions.Function1, int, java.lang.Object);
}
-keep,allowobfuscation class kotlin.collections.CollectionsKt
-keep,allowobfuscation class kotlin.collections.CollectionsKt__CollectionsJVMKt {
    java.util.List listOf(java.lang.Object);
}
-keep,allowobfuscation class kotlin.collections.CollectionsKt__IterablesKt {
    int collectionSizeOrDefault(java.lang.Iterable, int);
}
-keep,allowobfuscation class kotlin.collections.CollectionsKt__MutableCollectionsKt {
    boolean addAll(java.util.Collection, java.lang.Iterable);
}
-keep,allowobfuscation class kotlin.collections.CollectionsKt___CollectionsKt {
    java.lang.Object firstOrNull(java.util.List);
    java.lang.Object singleOrNull(java.util.List);
    java.lang.String joinToString$default(java.lang.Iterable, java.lang.CharSequence, java.lang.CharSequence, java.lang.CharSequence, int, java.lang.CharSequence, kotlin.jvm.functions.Function1, int, java.lang.Object);
}
-keep,allowobfuscation class kotlin.collections.MapsKt
-keep,allowobfuscation class kotlin.collections.MapsKt__MapsJVMKt {
    java.util.Map mapOf(kotlin.Pair);
}
-keep,allowobfuscation class kotlin.collections.MapsKt__MapsKt {
    java.util.Map emptyMap();
    java.util.Map mapOf(kotlin.Pair[]);
    java.util.Map mutableMapOf(kotlin.Pair[]);
    java.util.Map plus(java.util.Map, kotlin.Pair);
}
-keep,allowobfuscation class kotlin.collections.SetsKt
-keep,allowobfuscation class kotlin.collections.SetsKt__SetsKt {
    java.util.Set setOf(java.lang.Object[]);
}
-keep,allowobfuscation class kotlin.coroutines.Continuation
-keep,allowobfuscation class kotlin.coroutines.CoroutineContext
-keep,allowobfuscation class kotlin.coroutines.intrinsics.IntrinsicsKt
-keep,allowobfuscation class kotlin.coroutines.intrinsics.IntrinsicsKt__IntrinsicsKt {
    java.lang.Object getCOROUTINE_SUSPENDED();
}
# The test APK's suspend lambdas and continuations extend these and
# override create and invokeSuspend, which the app's coroutines call.
-keep,allowobfuscation class kotlin.coroutines.jvm.internal.BaseContinuationImpl {
    kotlin.coroutines.Continuation create(java.lang.Object, kotlin.coroutines.Continuation);
    java.lang.Object invokeSuspend(java.lang.Object);
}
-keep,allowobfuscation class kotlin.coroutines.jvm.internal.ContinuationImpl {
    <init>(kotlin.coroutines.Continuation);
}
-keep,allowobfuscation class kotlin.coroutines.jvm.internal.SuspendLambda {
    <init>(int, kotlin.coroutines.Continuation);
}
-keep,allowobfuscation class kotlin.coroutines.jvm.internal.Boxing {
    java.lang.Boolean boxBoolean(boolean);
}
-keep,allowobfuscation class kotlin.coroutines.jvm.internal.SpillingKt {
    java.lang.Object nullOutSpilledVariable(java.lang.Object);
}
-keep,allowobfuscation class kotlin.io.FilesKt
-keep,allowobfuscation class kotlin.io.FilesKt__FileReadWriteKt {
    byte[] readBytes(java.io.File);
    void writeBytes(java.io.File, byte[]);
}
-keep,allowobfuscation class kotlin.io.FilesKt__UtilsKt {
    boolean deleteRecursively(java.io.File);
}
# Implemented by the test APK's lambdas, which the app calls.
-keep,allowobfuscation class kotlin.jvm.functions.Function0 {
    java.lang.Object invoke();
}
-keep,allowobfuscation class kotlin.jvm.functions.Function1 {
    java.lang.Object invoke(java.lang.Object);
}
-keep,allowobfuscation class kotlin.jvm.functions.Function2 {
    java.lang.Object invoke(java.lang.Object, java.lang.Object);
}
-keep,allowobfuscation class kotlin.jvm.internal.DefaultConstructorMarker
-keep,allowobfuscation class kotlin.jvm.internal.Intrinsics {
    boolean areEqual(java.lang.Object, java.lang.Object);
    void throwUninitializedPropertyAccessException(java.lang.String);
}
-keep,allowobfuscation class kotlin.jvm.internal.Lambda {
    <init>(int);
}
-keep,allowobfuscation class kotlin.jvm.internal.Ref$BooleanRef {
    <init>();
    boolean element;
}
-keep,allowobfuscation class kotlin.jvm.internal.Ref$ObjectRef {
    <init>();
    java.lang.Object element;
}
-keep,allowobfuscation class kotlin.text.StringsKt
-keep,allowobfuscation class kotlin.text.StringsKt__StringsJVMKt {
    boolean endsWith$default(java.lang.String, java.lang.String, boolean, int, java.lang.Object);
    boolean startsWith$default(java.lang.String, java.lang.String, boolean, int, java.lang.Object);
}
-keep,allowobfuscation class kotlin.text.StringsKt___StringsKt {
    java.lang.String take(java.lang.String, int);
}

# kotlinx.coroutines: what the tests call, not the dispatcher loading.
-keep,allowobfuscation class kotlinx.coroutines.BuildersKt {
    java.lang.Object runBlockingK(kotlin.coroutines.CoroutineContext, kotlin.jvm.functions.Function2);
}
-keep,allowobfuscation class kotlinx.coroutines.CompletableDeferred {
    boolean complete(java.lang.Object);
}
-keep,allowobfuscation class kotlinx.coroutines.CompletableDeferredKt {
    kotlinx.coroutines.CompletableDeferred CompletableDeferred$default(kotlinx.coroutines.Job, int, java.lang.Object);
}
-keep,allowobfuscation class kotlinx.coroutines.CoroutineScope
-keep,allowobfuscation class kotlinx.coroutines.Deferred {
    java.lang.Object await(kotlin.coroutines.Continuation);
}
-keep,allowobfuscation class kotlinx.coroutines.DelayKt {
    java.lang.Object delay(long, kotlin.coroutines.Continuation);
}
-keep,allowobfuscation class kotlinx.coroutines.Dispatchers {
    kotlinx.coroutines.MainCoroutineDispatcher getMain();
}
-keep,allowobfuscation class kotlinx.coroutines.Job
-keep,allowobfuscation class kotlinx.coroutines.MainCoroutineDispatcher
-keep,allowobfuscation class kotlinx.coroutines.TimeoutKt {
    java.lang.Object withTimeout(long, kotlin.jvm.functions.Function2, kotlin.coroutines.Continuation);
    java.lang.Object withTimeoutOrNull(long, kotlin.jvm.functions.Function2, kotlin.coroutines.Continuation);
}
-keep,allowobfuscation class kotlinx.coroutines.flow.Flow
-keep,allowobfuscation class kotlinx.coroutines.flow.FlowKt {
    java.lang.Object first(kotlinx.coroutines.flow.Flow, kotlin.jvm.functions.Function2, kotlin.coroutines.Continuation);
}
-keep,allowobfuscation class kotlinx.coroutines.flow.StateFlow {
    java.lang.Object getValue();
}

# androidx.tracing: the androidx.test runner uses the app's copy.
-keep,allowobfuscation class androidx.tracing.Trace {
    void beginSection(java.lang.String);
    void endSection();
}
