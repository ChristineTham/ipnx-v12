// vendor-time fontsrv: render a TTF's latin range into a Plan 9 k8 subfont
// usage: mksubfont font.ttf pointsize outprefix
import Foundation
import CoreGraphics
import CoreText

let args = CommandLine.arguments
let data = try! Data(contentsOf: URL(fileURLWithPath: args[1])) as CFData
let size = CGFloat(Double(args[2])!)
let out = args[3]
let cgf = CGFont(CGDataProvider(data: data)!)!
let font = CTFontCreateWithGraphicsFont(cgf, size, nil, nil)

let ascent = Int(ceil(CTFontGetAscent(font)))
let descent = Int(ceil(CTFontGetDescent(font)))
let height = ascent + descent + 1
let lo = 0x20, hi = 0xFF
let n = hi - lo + 1
var widths: [Int] = [], glyphs: [CGGlyph] = []
for c in lo...hi {
  var ch = [UniChar(c)], g = [CGGlyph(0)]
  CTFontGetGlyphsForCharacters(font, &ch, &g, 1)
  var adv = CGSize.zero
  CTFontGetAdvancesForGlyphs(font, .default, g, &adv, 1)
  widths.append(max(1, Int(ceil(adv.width))))
  glyphs.append(g[0])
}
let total = widths.reduce(0, +)
let ctx = CGContext(data: nil, width: total, height: height, bitsPerComponent: 8,
  bytesPerRow: total, space: CGColorSpaceCreateDeviceGray(), bitmapInfo: 0)!
ctx.setAllowsAntialiasing(true)
ctx.setShouldAntialias(true)
ctx.setAllowsFontSmoothing(false)
ctx.setFillColor(gray: 1, alpha: 1)
var x = 0
for (i, g) in glyphs.enumerated() {
  var gg = g
  var pos = CGPoint(x: CGFloat(x), y: CGFloat(descent))
  CTFontDrawGlyphs(font, &gg, &pos, 1, ctx)
  x += widths[i]
}
let flipped = Data(bytes: ctx.data!, count: total * height)   // CG memory is already top-row-first
var outd = Data()
func f12(_ s: String) {
  var t = s
  while t.count < 11 { t = " " + t }
  outd.append((t + " ").data(using: .utf8)!)
}
for s in ["k8", "0", "0", String(total), String(height)] { f12(s) }
outd.append(flipped)
for s in [String(n), String(height), String(ascent)] { f12(s) }
var xoff = 0
for i in 0..<n {                    // n char entries
  var e = [UInt8](repeating: 0, count: 6)
  e[0] = UInt8(xoff & 0xff); e[1] = UInt8((xoff >> 8) & 0xff)
  e[2] = 0; e[3] = UInt8(height)    // top/bottom: full column
  e[4] = 0; e[5] = UInt8(min(widths[i], 255))
  outd.append(contentsOf: e)
  xoff += widths[i]
}
var term = [UInt8](repeating: 0, count: 6)   // n+1th entry: the end x
term[0] = UInt8(xoff & 0xff); term[1] = UInt8((xoff >> 8) & 0xff)
outd.append(contentsOf: term)
try! outd.write(to: URL(fileURLWithPath: out + ".subfont"))
print("\(height) \(ascent) \(total)")
