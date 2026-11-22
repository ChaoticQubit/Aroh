# Pod 24: TestFlight for the owner's iPhone

**Goal.** Q57: the owner installs Aroh from TestFlight, internal testing only. Bundle id `io.aroh.app`, team `GK729B5HZQ` (paid program), App ID registered with Push Notifications and HealthKit. One command builds a signed release archive, stamps a new build number and uploads it.

**Branch.** `chore/testflight` off master `2a900f2`.

## Research (2026-11-22, official sources, Xcode 27.2 build 27B52)

| Question | Finding | Source |
|---|---|---|
| Archive with automatic signing | `-allowProvisioningUpdates` lets xcodebuild talk to the Apple Developer website. For automatically signed targets it creates and updates profiles, app IDs and certificates. It needs a developer account added in Xcode's Accounts settings, or an App Store Connect key via `-authenticationKeyPath`, `-authenticationKeyID` and `-authenticationKeyIssuerID`. | `xcodebuild -help` (Xcode 27.2) |
| Upload: API key or Xcode account | Either. The same flags apply to `-exportArchive`. With the owner's team account signed into Xcode, no key is needed. On this Mac `IDEProvisioningTeamByIdentifier` lists `GK729B5HZQ` (not a free team), which shows the team was selected in Xcode once, not that the session is still live. Uploading needs the Account Holder, Admin, App Manager or Developer role. | `xcodebuild -help`, https://developer.apple.com/help/app-store-connect/manage-builds/upload-builds |
| ExportOptions.plist | `method` `app-store-connect` (`app-store` is deprecated), `destination` `upload` (default `export`), `teamID`, `signingStyle` `automatic`, `manageAppVersionAndBuildNumber` (default YES, Xcode may change the build number), `testFlightInternalTestingOnly` (the build cannot go to external TestFlight or the App Store), `uploadSymbols`. | `xcodebuild -help` "Available keys for -exportOptionsPlist", https://developer.apple.com/library/archive/technotes/tn2339/_index.html (the `-exportArchive` call), https://developer.apple.com/documentation/xcode/distributing-your-app-for-beta-testing-and-releases |
| Export compliance | `ITSAppUsesNonExemptEncryption` = NO when the app, including linked libraries, uses no encryption or only exempt encryption. Without the key, App Store Connect asks the questionnaire on every upload. HTTPS is the typical exempt case. | https://developer.apple.com/documentation/bundleresources/information-property-list/itsappusesnonexemptencryption, https://developer.apple.com/documentation/security/complying-with-encryption-export-regulations |
| `aps-environment` | Xcode sets the value from the current provisioning profile: development profile gives `development`, a distribution profile gives `production`. | https://developer.apple.com/documentation/bundleresources/entitlements/aps-environment |
| Build number | `CFBundleVersion` is one to three period-separated integers, digits and periods only. Bundle id plus version associate a build with the app record; the build string must identify the build uniquely. | https://developer.apple.com/documentation/bundleresources/information-property-list/cfbundleversion, https://developer.apple.com/help/app-store-connect/manage-builds/upload-builds |
| App record | Apps > + > New App: platform, name, primary language, bundle id, SKU, user access. Needs Account Holder, Admin or App Manager. | https://developer.apple.com/help/app-store-connect/create-an-app-record/add-a-new-app |
| Internal testers | TestFlight tab > + next to Internal Testing > name the group, tick "Enable automatic distribution"; then Invite Testers. Testers accept in the TestFlight app. | https://developer.apple.com/help/app-store-connect/test-a-beta-version/add-internal-testers |

## Decisions

| Topic | Decision | Reason |
|---|---|---|
| Script | `ios-release.sh testflight` mode; `ios-testflight.sh` only execs it. | One copy of the frontend, Rust staticlib and shim steps. |
| Build number | `AROH_BUILD_NUMBER`, default UTC `yyyymmddHHMM`; `manageAppVersionAndBuildNumber` NO. | Monotonic, no state, the uploaded number is the one the script prints. |
| No change in gen/apple | The stamped Info.plist is a temp copy passed as `INFOPLIST_FILE`; ExportOptions goes to `target/ios-release/testflight/`. Signing is set by build-setting overrides (`DEVELOPMENT_TEAM`, `CODE_SIGN_STYLE=Automatic`, `CODE_SIGN_IDENTITY=Apple Development`). | Nothing to restore, nothing to commit by accident. |
| `aps-environment` | Committed entitlements stay `development`. | Debug and release share one file. Automatic signing archives with a development profile, so a hardcoded `production` would not match it; the `app-store-connect` export re-signs with the distribution profile, which sets `production` (Apple doc above). |
| Export compliance | `ITSAppUsesNonExemptEncryption` false in Info.plist and project.yml. | Q57: HTTPS only. The app's TLS is rustls (reqwest `rustls` feature), standard TLS for HTTPS. |
| Internal only | `testFlightInternalTestingOnly` YES. | Q57: public release is a later decision. |
| API key | Optional, all three of `AROH_ASC_KEY_PATH`, `AROH_ASC_KEY_ID`, `AROH_ASC_ISSUER_ID` or none. | The key file stays outside the repo (`*.p8` is gitignored). |
| Local proof | `AROH_UNSIGNED=1` runs the same path with `CODE_SIGNING_ALLOWED=NO` and stops before export. | Signing creates certificates and profiles on the owner's account, so only the leader runs it. |

## Open items

- Signed archive and upload are not run by the pod. Verdict INCONCLUSIVE until the leader runs `sh apps/aroh/ios-testflight.sh`.
- Automatic signing archives with a development profile, which needs at least one device registered on the team. If none is, register the iPhone once (Xcode > Window > Devices and Simulators with the phone connected, or the developer portal).
- After the first upload, confirm `aps-environment` is `production` in the build (App Store Connect build metadata, or one export with `destination` `export` and `codesign -d --entitlements - Aroh.app`).
