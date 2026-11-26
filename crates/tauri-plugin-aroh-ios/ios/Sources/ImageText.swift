import CoreGraphics
import Foundation
import ImageIO
import Vision

/// What on-device OCR read from one image: the top candidate of every text observation in
/// reading order, one per line, and their mean confidence in 0...1 (0 when nothing was found).
struct ImageReading: Encodable {
  let text: String
  let confidence: Double
}

/// A downscaled JPEG copy of an image as base64, for a Claude transcription.
struct ImageCopy: Encodable {
  let jpeg: String
}

/// Reads an attached image's text with Apple Vision and makes the small JPEG copy sent to
/// Claude when the reading is unsure (Q91). Depends only on Foundation, ImageIO and Vision, so
/// its checks run on the Mac. Calls block; run them off the main queue. Never logs the image
/// or its text.
enum ImageText {
  /// Longest edge, in pixels, of the copy sent to Claude.
  static let maxEdge = 2576

  /// JPEG quality of the copy sent to Claude.
  static let jpegQuality = 0.8

  /// Recognizes the text in encoded image `data` (JPEG, PNG, HEIC and the rest ImageIO reads)
  /// at the accurate level with language correction, honouring the EXIF orientation.
  /// Output: the reading, or nil when `data` is not an image or Vision fails.
  static func read(_ data: Data) -> ImageReading? {
    guard let source = CGImageSourceCreateWithData(data as CFData, nil),
      let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
    else { return nil }
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = .accurate
    request.usesLanguageCorrection = true
    let handler = VNImageRequestHandler(cgImage: image, orientation: orientation(of: source), options: [:])
    do {
      try handler.perform([request])
    } catch {
      return nil
    }
    let observations = (request.results ?? []).sorted { a, b in
      let rowA = a.boundingBox.midY
      let rowB = b.boundingBox.midY
      if abs(rowA - rowB) > min(a.boundingBox.height, b.boundingBox.height) / 2 { return rowA > rowB }
      return a.boundingBox.minX < b.boundingBox.minX
    }
    let tops = observations.compactMap { $0.topCandidates(1).first }
    let text = tops.map(\.string).joined(separator: "\n")
    let confidence = tops.isEmpty ? 0 : tops.map { Double($0.confidence) }.reduce(0, +) / Double(tops.count)
    assert(confidence >= 0 && confidence <= 1, "Vision confidence is in 0...1")
    return ImageReading(text: text, confidence: confidence)
  }

  /// Re-encodes image `data` as a JPEG whose longest edge is at most `maxEdge`, upright (the
  /// orientation is applied), so HEIC and huge photos fit the Messages API.
  /// Output: the JPEG bytes, or nil when `data` is not an image.
  static func jpegCopy(_ data: Data) -> Data? {
    guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
    let options: [CFString: Any] = [
      kCGImageSourceCreateThumbnailFromImageAlways: true,
      kCGImageSourceCreateThumbnailWithTransform: true,
      kCGImageSourceThumbnailMaxPixelSize: maxEdge,
    ]
    guard let thumb = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { return nil }
    assert(max(thumb.width, thumb.height) <= maxEdge, "the copy fits the long-edge limit")
    let out = NSMutableData()
    guard let destination = CGImageDestinationCreateWithData(out, "public.jpeg" as CFString, 1, nil) else { return nil }
    CGImageDestinationAddImage(destination, thumb, [kCGImageDestinationLossyCompressionQuality: jpegQuality] as CFDictionary)
    guard CGImageDestinationFinalize(destination) else { return nil }
    return out as Data
  }

  /// The EXIF orientation of the first image in `source`, or up when it has none.
  private static func orientation(of source: CGImageSource) -> CGImagePropertyOrientation {
    let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any]
    let raw = (properties?[kCGImagePropertyOrientation] as? NSNumber)?.uint32Value ?? 1
    return CGImagePropertyOrientation(rawValue: raw) ?? .up
  }
}
