import os
import plistlib
import sys

ON = {"authorizationStatus": 2, "alertType": 1, "pushSettings": 63, "lockScreenSetting": 2, "notificationCenterSetting": 2, "showsInLockScreen": True, "showsInNotificationCenter": True}


def section(entries, bundle):
    """The archived section info of `bundle`, cloned from another app's entry when the app has none.
    Input: the plist's `sectionInfo` dict and the bundle id. Output: the decoded archive.
    A clone has the other app's id and display name replaced by `bundle` and `Aroh`."""
    if bundle in entries:
        return plistlib.loads(entries[bundle])
    source = "com.apple.Passbook" if "com.apple.Passbook" in entries else next(iter(entries))
    archive = plistlib.loads(entries[source])
    root = archive["$objects"][archive["$top"]["root"].data]
    names = {archive["$objects"][root[k].data] for k in ("sectionID", "appName", "displayName") if k in root and hasattr(root[k], "data")}
    archive["$objects"] = [bundle if o == source else ("Aroh" if isinstance(o, str) and o in names - {source} else o) for o in archive["$objects"]]
    return archive


def main():
    """Grants notifications to an app in a shut-down iOS simulator, which `simctl privacy` cannot.
    Input: device data dir and bundle id. Output: the patched BulletinBoard plist; prints the
    settings written. Test simulators only."""
    data, bundle = sys.argv[1], sys.argv[2]
    path = os.path.join(data, "Library/BulletinBoard/VersionedSectionInfo.plist")
    with open(path, "rb") as f:
        store = plistlib.load(f)
    archive = section(store["sectionInfo"], bundle)
    settings = [o for o in archive["$objects"] if isinstance(o, dict) and "authorizationStatus" in o]
    assert len(settings) == 1, f"expected one settings object, found {len(settings)}"
    settings[0].update(ON)
    store["sectionInfo"][bundle] = plistlib.dumps(archive, fmt=plistlib.FMT_BINARY)
    with open(path, "wb") as f:
        plistlib.dump(store, f, fmt=plistlib.FMT_BINARY)
    print(f"granted notifications to {bundle}: {ON}")


if __name__ == "__main__":
    main()
