# QML external renderer example

This package uses **real Quickshell/QtQuick** as an optional renderer. Install
Quickshell separately, add `"qml-example"` to the host `widgets` list, and restart.
The default widgets do not require Qt or Quickshell.

The example contains an animated shape, clickable controls, an editable text
field and a move/pin toggle. Its header moves the window when unpinned.
Initial dimensions, position, layer and pin state come from the manifest via
`RS_WWIDGET_*` environment variables. It creates its own layer-shell window,
with namespace `rs-wwidget-qml-example`, and manages its own pointer/keyboard input.
Quickshell provides its own QML live reloading.

`render_mode = "external"` means rs-wwidget supervises the process group without
creating a placeholder panel or parsing stdout. The renderer must create and
manage its own desktop surfaces. A normal application does not automatically
become a desktop widget just by selecting this mode. A missing executable or
crashed process is retried using the normal restart policy. No JSON heartbeats
are needed; stdin/stdout are null in this mode. The host captures a bounded stderr
tail, available through `rs-wwidget widget qml-example status`.

The host's input/keyboard/drag-grid/layout machinery is not applied to an
external window. The QML program can use the Quickshell/QtQuick APIs for
layouts, rich text, native input, animations, images, shaders and optional Qt
modules. Text-input and IME behavior belong to Qt/the compositor rather than the
host's raw keyboard protocol. The host does not enforce geometry/layer metadata
on an external program; this sample explicitly reads those environment variables.

Sources: [Quickshell guide](https://quickshell.org/docs/),
[PanelWindow](https://quickshell.org/docs/v0.3.2/types/Quickshell/PanelWindow/),
[WlrLayershell](https://quickshell.org/docs/v0.3.2/types/Quickshell.Wayland/WlrLayershell/).
