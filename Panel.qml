// Thread — a room on your bar.
//
// The glyph is a doorway. Closed, it is dim: you are hosting nothing. Open a
// room and it lights; when someone walks in, the count appears beside it, and
// your bar tells you a person arrived in a place you are serving off this
// machine. Click for the room itself.

import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

Panel {
  id: root
  moduleName: "io.pixygon.thread"
  ipcTarget: "io.pixygon.thread"
  manageIpc: false

  readonly property color foreground: bar ? bar.foreground : Color.foreground
  readonly property color urgent: bar ? bar.urgent : Color.urgent
  readonly property color dim: Qt.darker(foreground, 1.55)
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family
  readonly property color barForeground: bar ? bar.foreground : Color.foreground

  // Lit when a room is open, brighter still when somebody is in it.
  readonly property color glyphColor: !thread.serving
    ? Qt.darker(barForeground, 1.55)
    : (thread.count > 0 ? barForeground : Qt.darker(barForeground, 1.2))

  readonly property string doorway: "◈"
  readonly property string barLabel: thread.serving && thread.count > 0
    ? doorway + " " + thread.count
    : doorway

  readonly property string tooltip: {
    if (!thread.installed) return "Thread — helper not installed"
    if (!thread.serving) return "Thread — no room open. Click to open one."
    var head = (thread.title !== "" ? thread.title : "Your room") + " — " + thread.invite
    if (!thread.relayConnected) return head + "\n(not watching: the relay is unreachable)"
    return head + "\n" + thread.roster
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  onOpenedChanged: if (opened) {
    thread.refresh()
    thread.refreshRooms()
    Qt.callLater(function() { keyCatcher.forceActiveFocus() })
  }

  Service {
    id: thread
    settings: root.settings
  }

  IpcHandler {
    target: root.ipcTarget
    function open(): void { root.open() }
    function close(): void { root.close() }
    function show(): void { root.open() }
    function hide(): void { root.close() }
    function toggle(): void { root.toggle() }
    function refresh(): string { thread.refresh(); return "ok" }
    function room(): string { return thread.serving ? thread.invite : "" }
    function occupants(): string { return String(thread.count) }
  }

  BarIconButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: root.barLabel
    active: thread.serving && thread.count > 0
    tooltipText: root.tooltip
    onPressed: function(buttonCode) {
      // Middle-click walks straight in; that is the shortcut you learn second.
      if (buttonCode === Qt.MiddleButton) thread.openRoom(thread.room)
      else root.toggle()
    }
  }

  KeyboardPanel {
    id: panel
    anchorItem: button
    owner: root
    bar: root.bar
    open: root.opened
    focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(360))
    contentHeight: panel.fittedContentHeight(column.implicitHeight, Style.space(520))

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      onCloseRequested: root.close()
      onTextKey: function(t) {
        if (t === "o" || t === "O") thread.openRoom(thread.room)
        else if (t === "c" || t === "C") thread.copyInvite()
        else if (t === "n" || t === "N") thread.newRoom("A Room")
        else if (t === "x" || t === "X") thread.stop()
      }

      Flickable {
        id: flick
        anchors.fill: parent
        contentWidth: width
        contentHeight: column.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.VerticalFlick
        interactive: contentHeight > height

        Column {
          id: column
          width: flick.width
          spacing: Style.space(12)

          PanelHero {
            width: parent.width
            title: thread.serving ? (thread.title !== "" ? thread.title : "Your room") : "Thread"
            meta: !thread.installed
              ? "helper not installed"
              : (thread.serving
                 ? (thread.count === 1 ? "1 traveler inside" : thread.count + " travelers inside")
                 : "no room open")
            foreground: root.foreground
            fontFamily: root.fontFamily
            iconComponent: Component {
              Text {
                text: root.doorway
                color: root.glyphColor
                font.family: root.fontFamily
                font.pixelSize: Style.font.display
              }
            }
          }

          // The address, which is the whole point: something you can paste to
          // another person so they end up standing next to you.
          Text {
            visible: thread.serving && thread.invite !== ""
            width: parent.width
            text: thread.invite
            color: root.foreground
            font.family: root.fontFamily
            font.pixelSize: Style.font.bodySmall
            elide: Text.ElideMiddle
          }

          Text {
            visible: thread.serving
            width: parent.width
            text: thread.relayConnected ? thread.roster : "not watching — the relay is unreachable"
            color: root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.bodySmall
            wrapMode: Text.WordWrap
          }

          Text {
            visible: thread.actionStatus !== "" || thread.lastError !== ""
            width: parent.width
            text: thread.lastError !== "" ? thread.lastError : thread.actionStatus
            color: thread.lastError !== "" ? root.urgent : root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.bodySmall
            wrapMode: Text.WordWrap
          }

          RowLayout {
            width: parent.width
            spacing: Style.space(8)

            PanelActionButton {
              iconText: "󰐕"
              foreground: root.foreground
              fontFamily: root.fontFamily
              enabled: thread.installed && !thread.busy
              Layout.alignment: Qt.AlignVCenter
              onClicked: thread.newRoom("A Room")
              PanelToolTip { visible: parent.containsMouse; text: "New room"; fontFamily: root.fontFamily }
            }

            PanelActionButton {
              iconText: "󰈺"
              foreground: root.foreground
              fontFamily: root.fontFamily
              enabled: thread.installed && !thread.busy
              Layout.alignment: Qt.AlignVCenter
              onClicked: thread.openRoom(thread.room)
              PanelToolTip { visible: parent.containsMouse; text: "Step inside"; fontFamily: root.fontFamily }
            }

            PanelActionButton {
              iconText: "󰆏"
              foreground: root.foreground
              fontFamily: root.fontFamily
              enabled: thread.serving
              Layout.alignment: Qt.AlignVCenter
              onClicked: thread.copyInvite()
              PanelToolTip { visible: parent.containsMouse; text: "Copy the address"; fontFamily: root.fontFamily }
            }

            PanelActionButton {
              iconText: "󰅖"
              foreground: root.foreground
              fontFamily: root.fontFamily
              enabled: thread.serving
              Layout.alignment: Qt.AlignVCenter
              onClicked: thread.stop()
              PanelToolTip { visible: parent.containsMouse; text: "Close the room"; fontFamily: root.fontFamily }
            }
          }

          PanelSeparator {
            visible: thread.rooms.length > 0
            foreground: root.foreground
          }

          Column {
            visible: thread.rooms.length > 0
            width: parent.width
            spacing: Style.space(6)

            PanelSectionHeader {
              text: "YOUR ROOMS"
              foreground: root.foreground
              fontFamily: root.fontFamily
            }

            Repeater {
              model: thread.rooms
              delegate: Item {
                required property var modelData
                width: column.width
                height: Style.space(26)

                Text {
                  anchors.left: parent.left
                  anchors.verticalCenter: parent.verticalCenter
                  width: parent.width - Style.space(20)
                  text: (modelData.serving ? "◈  " : "◇  ") + String(modelData.title || modelData.room)
                  color: modelData.serving ? root.foreground : root.dim
                  font.family: root.fontFamily
                  font.pixelSize: Style.font.body
                  elide: Text.ElideRight
                }

                MouseArea {
                  anchors.fill: parent
                  hoverEnabled: true
                  cursorShape: Qt.PointingHandCursor
                  onClicked: thread.openRoom(String(modelData.room))
                }
              }
            }
          }

          Text {
            visible: !thread.installed
            width: parent.width
            text: "Install the helper:  yay -S omarchy-thread  —  or see the README"
            color: root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.bodySmall
            wrapMode: Text.WordWrap
          }
        }
      }
    }
  }
}
