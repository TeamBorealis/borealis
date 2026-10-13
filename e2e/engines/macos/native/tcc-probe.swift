// Reports (without prompting) whether the process chain that launched us holds the macOS privacy grants
// the agent-device macOS helper needs. TCC attributes a CLI to its responsible parent app, so this answers
// for the helper too. Prints: accessibility=<bool> screenrecording=<bool> postevent=<bool>
import ApplicationServices
import CoreGraphics
print("accessibility=\(AXIsProcessTrusted()) screenrecording=\(CGPreflightScreenCaptureAccess()) postevent=\(CGPreflightPostEventAccess())")
