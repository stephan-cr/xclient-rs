XClient
=======

This program is just a demonstration how to create a window on the
lowest level I could think of. It directly speaks to the X server via
a named pipe.

Resources
---------

- [Protocol description](https://www.x.org/releases/X11R7.7/doc/xproto/x11protocol.html)
- [Documentation for the X Window System](https://www.x.org/releases/current/doc/index.html)
- [XCB
  examples](https://www.x.org/releases/X11R7.5/doc/libxcb/tutorial/#gc),
  that might be used to understand how X11 works

Helpful snippets for development
--------------------------------

  ```shell
  Xephyr -retro :1
  ```

  ```shell
  Xnest -retro -cc 5 :1
  ```

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
- [DRI3](https://keithp.com/blogs/dri3_extension/)
- Find other interesting extensions
