// The state behind the widget.
//
// Everything real happens in the `omarchy-thread` binary — serving the world,
// holding the relay socket, checking a domain. This file only asks it what is
// true and turns the answer into properties. That split is deliberate: the
// hard parts stay testable without a compositor.

import QtQuick
import Quickshell
import Quickshell.Io

Item {
  id: root

  property var settings: ({})

  // What the room looks like from outside
  property bool installed: true
  property bool serving: false
  property string room: ""
  property string title: ""
  property string invite: ""
  property string manifestUrl: ""
  property bool relayConnected: false
  property var occupants: []
  property int count: 0
  property var rooms: []
  property string signedInAs: ""
  property string actionStatus: ""
  property string lastError: ""

  readonly property bool busy: statusProcess.running || actionProcess.running
  readonly property int pollIntervalSec: {
    var n = parseInt(String(settings && settings.pollIntervalSec !== undefined ? settings.pollIntervalSec : 3), 10)
    if (!isFinite(n)) n = 3
    return Math.max(1, Math.min(60, n))
  }

  // Who is inside, as a line of names. The relay hands us anonymous travelers
  // as bare ids, so the helper has already given them a readable stand-in.
  readonly property string roster: {
    var names = []
    for (var i = 0; i < occupants.length; i++) {
      var n = occupants[i] && occupants[i].name ? String(occupants[i].name) : ""
      if (n !== "") names.push(n)
    }
    if (names.length === 0) return serving ? "nobody here yet" : ""
    if (names.length <= 3) return names.join(", ")
    return names.slice(0, 3).join(", ") + " and " + (names.length - 3) + " more"
  }

  function refresh() {
    if (statusProcess.running) return
    statusProcess.command = ["omarchy-thread", "status"]
    statusProcess.running = true
  }

  function refreshRooms() {
    if (roomsProcess.running) return
    roomsProcess.command = ["omarchy-thread", "rooms"]
    roomsProcess.running = true
  }

  function applyStatus(raw) {
    var s = {}
    try { s = JSON.parse(String(raw || "{}")) } catch (e) { return }
    serving = s.serving === true
    room = String(s.room || "")
    title = String(s.title || "")
    invite = String(s.invite || "")
    manifestUrl = String(s.url || "")
    relayConnected = s.relay_connected === true
    occupants = s.occupants || []
    count = Number(s.count || 0)
    signedInAs = s.signed_in_as ? String(s.signed_in_as) : ""
  }

  // Every button is the same shape: run the helper, say what happened, look
  // again. Nothing in the UI pretends to know the outcome before it lands.
  function act(args, sayingWhat) {
    if (actionProcess.running) return
    actionStatus = sayingWhat || ""
    lastError = ""
    actionProcess.command = ["omarchy-thread"].concat(args)
    actionProcess.running = true
  }

  function openRoom(name) { act(name ? ["open", name] : ["open"], "opening the room…") }
  function newRoom(name) { act(["new", name || "A Room"], "making a room…") }
  function stop() { act(["stop"], "closing…") }
  function walkTo(address) { act(["open", address], "opening " + address + "…") }

  function copyInvite() {
    if (invite === "") return
    clipProcess.command = ["wl-copy", invite]
    clipProcess.running = true
    actionStatus = "address copied"
    statusClear.restart()
  }

  Timer {
    id: poll
    interval: root.pollIntervalSec * 1000
    repeat: true
    running: true
    triggeredOnStart: true
    onTriggered: root.refresh()
  }

  Timer {
    id: statusClear
    interval: 2400
    repeat: false
    onTriggered: root.actionStatus = ""
  }

  // A room takes a moment to come up; look again a few times rather than
  // making the reader wait for the next poll.
  Timer {
    id: settle
    property int ticks: 0
    interval: 700
    repeat: true
    running: false
    onTriggered: {
      settle.ticks += 1
      root.refresh()
      if (settle.ticks >= 6) { settle.ticks = 0; settle.running = false }
    }
  }

  Process {
    id: statusProcess
    running: false
    command: []
    stdout: StdioCollector { id: statusOut; waitForEnd: true }
    stderr: StdioCollector { id: statusErr; waitForEnd: true }
    onExited: function(exitCode) {
      if (exitCode === 0) {
        root.installed = true
        root.applyStatus(statusOut.text)
      } else if (String(statusErr.text).indexOf("No such file") >= 0) {
        root.installed = false
      }
    }
  }

  Process {
    id: roomsProcess
    running: false
    command: []
    stdout: StdioCollector { id: roomsOut; waitForEnd: true }
    onExited: function(exitCode) {
      if (exitCode !== 0) return
      try { root.rooms = JSON.parse(String(roomsOut.text || "{}")).rooms || [] } catch (e) { root.rooms = [] }
    }
  }

  Process {
    id: actionProcess
    running: false
    command: []
    stdout: StdioCollector { id: actionOut; waitForEnd: true }
    stderr: StdioCollector { id: actionErr; waitForEnd: true }
    onExited: function(exitCode) {
      var err = String(actionErr.text || "").trim()
      if (exitCode === 0) {
        root.lastError = ""
        root.actionStatus = ""
      } else {
        // The helper's last line is the one written for a person to read.
        var lines = err.split("\n").filter(function(l) { return l.trim() !== "" })
        root.lastError = lines.length ? lines[lines.length - 1].replace(/^✗ /, "") : "that didn't work"
        root.actionStatus = ""
      }
      settle.ticks = 0
      settle.restart()
      root.refreshRooms()
    }
  }

  Process {
    id: clipProcess
    running: false
    command: []
  }

  Component.onCompleted: root.refreshRooms()
}
