import AuthenticationServices
import CryptoKit
import UIKit

/// Native Sign in with Apple for the `apple_sign_in` command. `start` shows Apple's sheet with
/// the SHA-256 (hex) of the raw nonce the Rust side made, and calls back once with the identity
/// token as UTF-8 text or an error whose code is `cancelled`, `failed` or `no_token`.
final class AppleSignIn: NSObject, ASAuthorizationControllerDelegate, ASAuthorizationControllerPresentationContextProviding {
  static let shared = AppleSignIn()
  private var done: ((Result<String, AppleSignInError>) -> Void)?

  /// Starts one sign-in; a second call while one runs fails the first as cancelled.
  func start(nonce: String, completion: @escaping (Result<String, AppleSignInError>) -> Void) {
    DispatchQueue.main.async {
      self.done?(.failure(AppleSignInError(code: "cancelled", message: "Sign in with Apple was replaced by a new attempt.")))
      self.done = completion
      let request = ASAuthorizationAppleIDProvider().createRequest()
      request.requestedScopes = [.email]
      request.nonce = SHA256.hash(data: Data(nonce.utf8)).map { String(format: "%02x", $0) }.joined()
      let controller = ASAuthorizationController(authorizationRequests: [request])
      controller.delegate = self
      controller.presentationContextProvider = self
      controller.performRequests()
    }
  }

  /// The key window Apple's sheet attaches to.
  func presentationAnchor(for controller: ASAuthorizationController) -> ASPresentationAnchor {
    let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
    return scenes.flatMap { $0.windows }.first { $0.isKeyWindow } ?? ASPresentationAnchor()
  }

  /// Apple approved: hands back the identity token, or `no_token` when Apple sent none.
  func authorizationController(controller: ASAuthorizationController, didCompleteWithAuthorization authorization: ASAuthorization) {
    let credential = authorization.credential as? ASAuthorizationAppleIDCredential
    guard let data = credential?.identityToken, let token = String(data: data, encoding: .utf8) else {
      return finish(.failure(AppleSignInError(code: "no_token", message: "Apple returned no identity token.")))
    }
    finish(.success(token))
  }

  /// Apple refused or the owner closed the sheet: `cancelled` or `failed` with Apple's reason.
  func authorizationController(controller: ASAuthorizationController, didCompleteWithError error: Error) {
    let cancelled = (error as? ASAuthorizationError)?.code == .canceled
    finish(.failure(AppleSignInError(code: cancelled ? "cancelled" : "failed", message: cancelled ? "Sign in with Apple was cancelled." : error.localizedDescription)))
  }

  /// Calls the waiting completion once and forgets it.
  private func finish(_ result: Result<String, AppleSignInError>) {
    let callback = done
    done = nil
    callback?(result)
  }
}

/// A Sign in with Apple failure: a stable `code` and an owner-readable `message`.
struct AppleSignInError: Error {
  let code: String
  let message: String
}
