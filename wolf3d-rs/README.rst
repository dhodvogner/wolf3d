Wolfenstein 3D
==============

This repository contains the original Wolfenstein 3D source release and an in-progress Rust modernization.

Rust modernization (macroquad)
------------------------------

A modern rewrite scaffold is available at:

- ``/home/runner/work/wolf3d/wolf3d/wolf3d-rs``

Current implementation focus:

- macroquad-driven game loop
- software raycasting renderer (CPU DDA raycaster)
- ECS-style world/entity storage
- finite-state-machine based game state control
- observer/event bus wiring
- command pattern for input actions
- object pool for reusable objects
- compatibility loader for original ``MAPHEAD``, ``GAMEMAPS``, and ``VSWAP`` data files
- original map semantics for floors/walls/doors
- basic enemy extraction and runtime AI/shooting loop

Data compatibility
------------------

The runtime auto-detects one supported game-data extension:

- ``WL6`` (registered Wolf3D)
- ``WL1`` (shareware Wolf3D)
- ``SDM`` or ``SOD`` (Spear of Destiny variants)

Place data files in one of these locations:

- ``/home/runner/work/wolf3d/wolf3d/wolf3d-rs/data``
- ``/home/runner/work/wolf3d/wolf3d/wolf3d-rs``
- a custom directory via ``WOLF3D_DATA_DIR``

Required files for the chosen extension:

- ``MAPHEAD.<EXT>``
- ``GAMEMAPS.<EXT>``
- ``VSWAP.<EXT>``

Build and run
-------------

::

  cd /home/runner/work/wolf3d/wolf3d/wolf3d-rs
  cargo run

Controls
--------

- ``W/S`` or Up/Down: move forward/backward
- ``A/D`` or Left/Right: turn
- ``E`` or ``Space``: use/toggle door in front
- ``Left Ctrl`` or ``Enter``: fire
- ``Esc``: pause/resume
- ``Q``: quit
