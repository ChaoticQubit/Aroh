import CoreGraphics
import CoreText
import Foundation
import ImageIO

var passed = 0
var failed: [String] = []

/// Records check `name` as passed when `ok`. Prints only the name on failure, never image text.
func check(_ name: String, _ ok: Bool) {
  if ok { passed += 1 } else { failed.append(name) }
}

/// Encodes a white `width` x `height` image with black `lines` drawn on it as `type` (a UTI)
/// carrying EXIF `orientation`; orientation 6 stores the pixels turned a quarter circle
/// (`height` wide, `width` tall), as a phone held sideways stores a photo.
/// Output: the encoded bytes.
func picture(_ lines: [String], width: Int, height: Int, type: String, orientation: UInt32 = 1) -> Data {
  let space = CGColorSpaceCreateDeviceRGB()
  let turned = orientation == 6
  let context = CGContext(
    data: nil, width: turned ? height : width, height: turned ? width : height, bitsPerComponent: 8, bytesPerRow: 0, space: space,
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
  context.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
  context.fill(CGRect(x: 0, y: 0, width: max(width, height), height: max(width, height)))
  if turned {
    context.translateBy(x: CGFloat(height), y: 0)
    context.rotate(by: .pi / 2)
  }
  let font = CTFontCreateWithName("Helvetica" as CFString, CGFloat(height) / CGFloat(lines.count * 3), nil)
  for (i, line) in lines.enumerated() {
    let attributes: [NSAttributedString.Key: Any] = [
      NSAttributedString.Key(kCTFontAttributeName as String): font,
      NSAttributedString.Key(kCTForegroundColorAttributeName as String): CGColor(red: 0, green: 0, blue: 0, alpha: 1),
    ]
    let ctLine = CTLineCreateWithAttributedString(NSAttributedString(string: line, attributes: attributes))
    context.textPosition = CGPoint(x: CGFloat(width) / 20, y: CGFloat(height) * (1 - CGFloat(i + 1) / CGFloat(lines.count + 1)))
    CTLineDraw(ctLine, context)
  }
  let out = NSMutableData()
  let destination = CGImageDestinationCreateWithData(out, type as CFString, 1, nil)!
  CGImageDestinationAddImage(destination, context.makeImage()!, [kCGImagePropertyOrientation: orientation] as CFDictionary)
  _ = CGImageDestinationFinalize(destination)
  return out as Data
}

let page = picture(["Chapter one", "The joy of x"], width: 1200, height: 600, type: "public.jpeg")
let reading = ImageText.read(page)
check("readable_text_is_found", reading?.text.lowercased().contains("chapter one") == true)
check("lines_keep_reading_order", reading.map { $0.text.lowercased().components(separatedBy: "\n") } == ["chapter one", "the joy of x"])
check("clear_text_is_confident", (reading?.confidence ?? 0) >= 0.75)

let blank = picture([], width: 400, height: 400, type: "public.png")
let empty = ImageText.read(blank)
check("blank_image_reads_nothing", empty?.text == "" && empty?.confidence == 0)
check("non_image_is_nil", ImageText.read(Data("not an image".utf8)) == nil)

let huge = picture(["Big"], width: 6000, height: 3000, type: "public.png")
let copy = ImageText.jpegCopy(huge).flatMap { CGImageSourceCreateWithData($0 as CFData, nil) }
let copyImage = copy.flatMap { CGImageSourceCreateImageAtIndex($0, 0, nil) }
check("copy_is_jpeg", copy.flatMap { CGImageSourceGetType($0) as String? } == "public.jpeg")
check("copy_fits_long_edge", copyImage.map { max($0.width, $0.height) <= ImageText.maxEdge } == true)

let rotated = picture(["Upright words"], width: 2400, height: 600, type: "public.jpeg", orientation: 6)
check("rotated_photo_reads", ImageText.read(rotated)?.text.lowercased().contains("upright words") == true)

print("image text checks: \(passed) passed, \(failed.count) failed")
for name in failed { print("failed: \(name)") }
exit(failed.isEmpty ? 0 : 1)
