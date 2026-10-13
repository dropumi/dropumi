# dropumi mobile assets

Source SVGs are in `src/`. Colours: Deep Navy `#0B1F3A`, Light Teal `#3CC4D4`, white. See `../GUIDELINES.md`.

## iOS
- `ios/AppIcon-1024.png` is the App Store and Xcode icon. It's a full-bleed square with no transparency and no pre-rounded corners, because iOS applies the mask.
- `AppIcon-1024-dark.png` and `AppIcon-1024-tinted.png` are for the iOS 18 dark and tinted appearances (Xcode: *Any / Dark / Tinted* slots).

## Android
- `android/adaptive/mipmap-*/ic_launcher_foreground.png` (108 dp, mark inside the 66 dp safe zone) and `ic_launcher_monochrome.png` (for Android 13+ themed icons).
- Background is the solid colour `#0B1F3A`. Add to `res/values/ic_launcher_background.xml`:
  `<resources><color name="ic_launcher_background">#0B1F3A</color></resources>`
- `res/mipmap-anydpi-v26/ic_launcher.xml`:
  ```xml
  <adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
      <background android:drawable="@color/ic_launcher_background"/>
      <foreground android:drawable="@mipmap/ic_launcher_foreground"/>
      <monochrome android:drawable="@mipmap/ic_launcher_monochrome"/>
  </adaptive-icon>
  ```
- `android/legacy/mipmap-*/ic_launcher.png` is the fallback for Android 7 and older.
- `android/notification/drawable-*/ic_stat_dropumi.png` is the white status-bar icon (Android tints it).

## Splash
- `splash/splash-symbol-1152.png` is the transparent symbol for the Android 12+ splash screen (icon area 1152 px, safe circle 768 px). Use navy as the splash background colour.
- `splash/splash-portrait-1284x2778.png` is a full portrait splash with the wordmark, for iOS launch screens or older Android.

## Store
- `store/play-store-icon-512.png` (Google Play icon) and `store/play-feature-graphic-1024x500.png`.
