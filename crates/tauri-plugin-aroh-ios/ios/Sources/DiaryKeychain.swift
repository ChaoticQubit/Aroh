import Foundation
import Security

/// The diary key in the iCloud Keychain (Q85): generic password items of service
/// `aroh.diary`, account `v{version}`, synchronizable and readable after first unlock.
/// Holds the key as base64 text from Rust; never logs it. Returns nil when absent.
enum DiaryKeychain {
  static let service = "aroh.diary"

  /// The base query for key `version`: service, account and the synchronizable flag,
  /// which every get, add and delete must carry to reach the iCloud item.
  static func query(_ version: Int) -> [String: Any] {
    return [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: "v\(version)",
      kSecAttrSynchronizable as String: kCFBooleanTrue as Any,
    ]
  }

  /// Reads key `version`. Returns its base64 text, nil when absent, or the OSStatus on error.
  static func get(_ version: Int) -> Result<String?, KeychainError> {
    var q = query(version)
    q[kSecReturnData as String] = kCFBooleanTrue
    q[kSecMatchLimit as String] = kSecMatchLimitOne
    var item: CFTypeRef?
    let status = SecItemCopyMatching(q as CFDictionary, &item)
    if status == errSecItemNotFound { return .success(nil) }
    guard status == errSecSuccess, let data = item as? Data, let text = String(data: data, encoding: .utf8) else {
      return .failure(KeychainError(status: status))
    }
    return .success(text)
  }

  /// Stores `key` (base64) as key `version`, only when no item exists, so a key another
  /// device already synced is never overwritten. Returns false when one was there.
  static func put(_ version: Int, key: String) -> Result<Bool, KeychainError> {
    var q = query(version)
    q[kSecValueData as String] = Data(key.utf8)
    q[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock
    let status = SecItemAdd(q as CFDictionary, nil)
    if status == errSecDuplicateItem { return .success(false) }
    guard status == errSecSuccess else { return .failure(KeychainError(status: status)) }
    return .success(true)
  }

  /// Deletes key `version`. Succeeds when it was already absent.
  static func delete(_ version: Int) -> Result<Void, KeychainError> {
    let status = SecItemDelete(query(version) as CFDictionary)
    guard status == errSecSuccess || status == errSecItemNotFound else { return .failure(KeychainError(status: status)) }
    return .success(())
  }
}

/// A keychain failure carrying only the OSStatus code, never item data.
struct KeychainError: Error {
  let status: OSStatus
  var message: String { return "Keychain error \(status)." }
}
