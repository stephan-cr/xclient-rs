XClient
=======

This program is just a demonstration how to create a window on the
lowest level I could think of. It directly speaks to the X server via
a named pipe.

[Protocol description](https://www.x.org/releases/X11R7.7/doc/xproto/x11protocol.html)
[Documentation for the X Window System](https://www.x.org/releases/current/doc/index.html)

```shell
$ Xnest -retro -cc 5 :1
```

- [XCB
  examples](https://www.x.org/releases/X11R7.5/doc/libxcb/tutorial/#gc),
  that might be used to understand how X11 works

HOWTO
-----

- list extensions:

```shell
xdpyinfo -queryExtensions
```

TODO
----

- implement [X Nonrectangular Window Shape Extension
  Protocol](https://www.x.org/releases/current/doc/xextproto/shape.html),
  which sounds funny
- check why it doesn't work with Xephyr

  ```shell
  Xephyr -retro :1
  ```
