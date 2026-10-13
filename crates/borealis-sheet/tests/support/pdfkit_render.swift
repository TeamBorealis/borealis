// Renders one page of a PDF the way Apple PDFKit shows it (Preview, Safari): page content plus
// PDFKit's own drawing of the form widgets. Writes an 8-bit grayscale PNG at `dpi`.
// Usage: pdfkit_render <in.pdf> <page index> <dpi> <out.png>
import AppKit
import PDFKit

let args = CommandLine.arguments
guard args.count == 5, let doc = PDFDocument(url: URL(fileURLWithPath: args[1])),
    let page = doc.page(at: Int(args[2]) ?? -1), let dpi = Double(args[3])
else {
    FileHandle.standardError.write("usage: pdfkit_render <in.pdf> <page index> <dpi> <out.png>\n".data(using: .utf8)!)
    exit(2)
}
let scale = CGFloat(dpi / 72)
let box = page.bounds(for: .mediaBox)
let (w, h) = (Int((box.width * scale).rounded()), Int((box.height * scale).rounded()))
let ctx = CGContext(
    data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0,
    space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue)!
ctx.setFillColor(gray: 1, alpha: 1)
ctx.fill(CGRect(x: 0, y: 0, width: w, height: h))
ctx.scaleBy(x: scale, y: scale)
page.draw(with: .mediaBox, to: ctx)
let png = NSBitmapImageRep(cgImage: ctx.makeImage()!).representation(using: .png, properties: [:])!
try! png.write(to: URL(fileURLWithPath: args[4]))
