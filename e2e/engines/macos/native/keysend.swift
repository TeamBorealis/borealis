// Posts one key press with modifiers: keysend <virtualKeyCode> [command] [shift] [option] [control]
// Used for chords (Cmd+S) that agent-device's `type` cannot send. Needs Accessibility, like the helper.
import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count >= 2, let code = UInt16(args[1]) else {
  FileHandle.standardError.write(Data("usage: keysend <keycode> [command] [shift] [option] [control]\n".utf8))
  exit(2)
}
var flags = CGEventFlags()
for name in args.dropFirst(2) {
  switch name {
  case "command": flags.insert(.maskCommand)
  case "shift": flags.insert(.maskShift)
  case "option": flags.insert(.maskAlternate)
  case "control": flags.insert(.maskControl)
  default: exit(2)
  }
}
let source = CGEventSource(stateID: .privateState)
for isDown in [true, false] {
  guard let event = CGEvent(keyboardEventSource: source, virtualKey: code, keyDown: isDown) else { exit(3) }
  event.flags = flags
  event.post(tap: .cghidEventTap)
  usleep(25_000)
}
