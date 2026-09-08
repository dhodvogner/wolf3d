Wolfenstein 3D
==============

This repository contains the original Wolfenstein 3D source release and an in-progress Rust modernization.

Rust modernization (macroquad)
------------------------------

A modern rewrite scaffold is available at:

- ``/home/runner/work/wolf3d/wolf3d/wolf3d-rs``

It currently includes:

- macroquad-driven game loop
- software raycasting renderer (CPU ray stepping + column projection)
- ECS-style world/entity storage
- finite-state-machine based game state control
- observer/event bus wiring
- command pattern for input actions
- object pool for reusable particles

Build and run
-------------

::

  cd /home/runner/work/wolf3d/wolf3d/wolf3d-rs
  cargo run
