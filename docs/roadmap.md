# Engine milestones

1. **Player.** Present an interactive native window with reusable GPU resources, input, resize/minimize handling and frame timing. Retain deterministic offscreen rendering as a test oracle.
2. **Assets and world.** Import real mesh data, give assets stable IDs, load a saved scene, and reuse geometry between instances.
3. **Animation and replay.** Add skeletal clips, sockets, authored cues and seeking. Verify game-owned fixtures through adapters independently of engine source.
4. **Editor and builder.** Move an object and camera, edit a cue, undo, save, reopen and build the same scene into a standalone player.
5. **Platform qualification.** Develop native Apple Metal and browser WebGPU backends alongside Vulkan. Track compiler coverage, software-driver tests, and real device sessions separately.
6. **Prove reuse.** Ship two independent game clients including a non-card sample with movement and collision, without game-specific concepts in the engine API.
