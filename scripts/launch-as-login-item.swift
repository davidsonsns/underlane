// Launch an app the way macOS opens login items: an 'oapp' Apple event whose keyAEPropData is
// keyAELaunchedAsLogInItem ('lgit'), delivered by LaunchServices, without activating the app.
// Usage: swift scripts/launch-as-login-item.swift /path/to/App.app
import AppKit

let path = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "\(NSHomeDirectory())/Applications/Underlane.app"
let event = NSAppleEventDescriptor(
    eventClass: AEEventClass(kCoreEventClass), eventID: AEEventID(kAEOpenApplication),
    targetDescriptor: nil, returnID: AEReturnID(kAutoGenerateReturnID), transactionID: AETransactionID(kAnyTransactionID))
event.setParam(NSAppleEventDescriptor(enumCode: OSType(keyAELaunchedAsLogInItem)), forKeyword: AEKeyword(keyAEPropData))
let config = NSWorkspace.OpenConfiguration()
config.appleEvent = event
config.activates = false
config.addsToRecentItems = false
let done = DispatchSemaphore(value: 0)
NSWorkspace.shared.openApplication(at: URL(fileURLWithPath: path), configuration: config) { app, error in
    if let error { print("launch failed: \(error)") } else { print("launched \(path) as a login item (pid \(app?.processIdentifier ?? -1))") }
    done.signal()
}
while done.wait(timeout: .now() + 0.05) == .timedOut { RunLoop.current.run(until: Date().addingTimeInterval(0.05)) }
