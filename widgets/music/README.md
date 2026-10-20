# Music widget

Displays status from an available MPRIS-compatible desktop media player and
uses the session D-Bus for playback controls. The widget requires the `gio-2.0`
development package to build; it does not require `playerctl`.

The executable also accepts one control action:

```sh
./widgets/music/music --play-pause
./widgets/music/music --next
./widgets/music/music --previous
```

The widget is read-only over the frame protocol. Invoke the executable's CLI
separately to send an action.
