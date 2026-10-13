// Posts a real left mouse click at screen point (x, y): click <x> <y> [count]
// agent-device's XCUI press does not reach the Borealis webview, so the engine clicks with CGEvents (needs Accessibility).
import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count >= 3, let x = Double(args[1]), let y = Double(args[2]) else {
  FileHandle.standardError.write(Data("usage: click <x> <y> [count]\n".utf8))
  exit(2)
}
let count = args.count > 3 ? Int64(args[3]) ?? 1 : 1
let point = CGPoint(x: x, y: y)
let source = CGEventSource(stateID: .privateState)
func post(_ type: CGEventType, _ clickState: Int64) {
  guard let e = CGEvent(mouseEventSource: source, mouseType: type, mouseCursorPosition: point, mouseButton: .left) else { exit(3) }
  e.setIntegerValueField(.mouseEventClickState, value: clickState)
  e.post(tap: .cghidEventTap)
  usleep(30_000)
}
post(.mouseMoved, 0)
usleep(120_000)
for n in 1...count {
  post(.leftMouseDown, n)
  post(.leftMouseUp, n)
}
