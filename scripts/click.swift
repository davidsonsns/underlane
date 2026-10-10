// Post a left mouse click at screen coordinates (points). Usage: swift scripts/click.swift x y
import CoreGraphics
import Foundation
let x = Double(CommandLine.arguments[1])!, y = Double(CommandLine.arguments[2])!
let p = CGPoint(x: x, y: y)
let src = CGEventSource(stateID: .hidSystemState)
let saved = CGEvent(source: nil)?.location
CGEvent(mouseEventSource: src, mouseType: .mouseMoved, mouseCursorPosition: p, mouseButton: .left)?.post(tap: .cghidEventTap)
usleep(80_000)
CGEvent(mouseEventSource: src, mouseType: .leftMouseDown, mouseCursorPosition: p, mouseButton: .left)?.post(tap: .cghidEventTap)
usleep(60_000)
CGEvent(mouseEventSource: src, mouseType: .leftMouseUp, mouseCursorPosition: p, mouseButton: .left)?.post(tap: .cghidEventTap)
usleep(80_000)
if let saved { CGEvent(mouseEventSource: src, mouseType: .mouseMoved, mouseCursorPosition: saved, mouseButton: .left)?.post(tap: .cghidEventTap) }
