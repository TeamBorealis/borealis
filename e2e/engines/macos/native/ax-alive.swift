// Reports whether each process answers accessibility: ax-alive <pid>... prints "pid=<n> alive=<bool>" per pid.
// A process that does not reply within the timeout (kAXErrorCannotComplete) is not alive; any other answer, an
// error included, means it replied. Needs Accessibility, like the other helpers.
import ApplicationServices
import Foundation

let replyTimeoutSeconds: Float = 2.0
for arg in CommandLine.arguments.dropFirst() {
  guard let pid = pid_t(arg) else { continue }
  let app = AXUIElementCreateApplication(pid)
  AXUIElementSetMessagingTimeout(app, replyTimeoutSeconds)
  var role: CFTypeRef?
  let err = AXUIElementCopyAttributeValue(app, kAXRoleAttribute as CFString, &role)
  print("pid=\(pid) alive=\(err != .cannotComplete)")
}
