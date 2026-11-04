# Data: projects, documents, identities and cooked output

**Decision.** Everything a person authors is a canonical JSON text file that diffs and merges line by line.
Everything the runtime loads is cooked from those files into per-platform binary packages whose file names are
their content hashes. References use permanent random ids, never paths. A published file never changes; a new
version is a new file.

This page defines the files. [Editor](03-editor.md) says who writes them, [runtime](02-runtime.md) who reads the
cooked results.

## 1. A project on disk

```text
my-game/
  ganbate.project.json            the manifest (below)
  content/                    everything authored or imported, committed
    models/lantern.glb        a source asset ...
    models/lantern.glb.meta.json   ... and its sidecar: id and import settings
    scenes/harbour.scene.json engine documents carry their own id
    prefabs/lantern.prefab.json
    looks/night.look.json + .meta.json
  schemas/game.schema.json    the game's component kinds
  editor/                     the game's editor plugins (JavaScript modules)
  .ganbate/                       local derived data, ignored by git: in M1 the editor's cooked previews; later the
                              import cache, thumbnails and editor state
  build/                      cooked packages per platform (ignored by git)
  publish/                    content-addressed files the game ships (committed or uploaded: the game decides)
```

`ganbate.project.json`:

```json
{
  "ganbate": "project",
  "content": [
    "content"
  ],
  "editor": [
    "editor"
  ],
  "id": "4f0c1d2e9a8b7c6d5e4f30211a2b3c4d",
  "name": "Example",
  "platforms": {
    "desktop": {
      "textures": "rgba8"
    },
    "web": {
      "compact": true,
      "textures": "source"
    }
  },
  "schemas": [
    "schemas"
  ],
  "startup": "8e1f4b2c0d3a5e6f7a8b9c0d1e2f3a4b",
  "version": 1
}
```

The manifest names folders and the scene the editor opens first (`startup`). `platforms` is accepted and kept,
but no tool reads it yet: per-platform cook settings take effect with `ganbate-build cook-project` (planned, M2). Engine
version requirements and plugin lists join it when they exist; unknown keys are refused so typos surface.

## 2. Identity

| Thing | Id | Where it is written |
| --- | --- | --- |
| Asset (scene, prefab, model, texture, Look, sound...) | 128-bit random, 32 lowercase hex digits | engine documents: their own `"id"`; any other file: its `<file>.meta.json` sidecar |
| Entity | 64-bit random, 16 hex digits | the key of the entity in its scene or prefab |
| Entity shown by a prefab instance | `derived(instance id, prefab entity id)`: first 8 bytes of SHA-256 of `"<instance>/<inner>"` | never written; recomputed on load |
| Cooked or published file | SHA-256 of its bytes | its file name and the package manifest |

Random ids mean two people adding things on separate branches never collide, and renaming or moving a file never
breaks a reference. Move the source and its sidecar together. Studio creates missing sidecars when it opens the project or when an artist uses **Rescan** in Assets. It
refuses to open a project in which two files share one id (a copied file), naming both; in M1 the copy's id is
changed by hand (an offer to give it a new id is planned). A sidecar is small and committed beside its file:

```json
{
  "ganbate": "meta",
  "id": "8c2d...",
  "import": {},
  "kind": "model",
  "version": 1
}
```

`import` will hold import settings when the importer gains them (M2: fix-ups such as UV repair and unit scale); M1
writes it empty and reads nothing from it.

## 3. Canonical JSON

One value has one spelling, so a save that changes one field changes one line:

- Object keys one per line in byte order, two-space indent. Entities are an object keyed by id, so two people
  adding entities touch different lines.
- Arrays of numbers, booleans and nulls on one line (`[0, 1.5, 0]`): vectors, colours, matrices. Other arrays one
  element per line.
- Numbers in their shortest round-trip spelling; a value that is exactly a 32-bit float in the 32-bit spelling
  (`0.1`, not `0.10000000149011612`). No exponent between 1e-6 and 1e21.
- UTF-8, only JSON's required escapes, a final newline. Reading and writing canonical text gives the same bytes.

`ganbate_core::json::to_canonical` writes it. The reader is bounded (size, depth, value count) and refuses duplicate
keys. A file a person edits by hand is reformatted on the next save; the editor never fails on non-canonical
input.

**Why JSON and not a custom text format:** every tool, browser and script reads it, the engine already had a
bounded reader, and the canonical rules recover most of the merge-friendliness of line formats such as `.tscn`.

## 4. Documents

Every document starts with `"ganbate": "<kind>"` and `"version": n`. A reader refuses versions newer than it knows
and migrates older ones on load; the next save writes the current version. (The Look predates these rules: see its
row below.)

### Scene and prefab

```json
{
  "ganbate": "scene",
  "entities": {
    "0f1e2d3c4b5a6978": {
      "name": "Pier lights",
      "prefab": {
        "asset": "c4a1...",
        "overrides": {
          "1122334455667788": {
            "removed": true
          },
          "9a8b7c6d5e4f3021": {
            "components": {
              "light": {
                "intensity": 2.5
              }
            }
          }
        }
      }
    },
    "2b8f0c71d4e6a935": {
      "components": {
        "mesh": {
          "model": "7d0e...",
          "node": "Lantern"
        },
        "transform": {
          "translation": [4.5, 0, -2]
        }
      },
      "name": "Lantern 3",
      "order": "V",
      "parent": "0f1e2d3c4b5a6978"
    }
  },
  "id": "8e1f4b2c0d3a5e6f7a8b9c0d1e2f3a4b",
  "name": "Harbour",
  "version": 1
}
```

- An entity has a `name`, optionally a `parent`, an `order` key among its siblings, `components` and `prefab`.
- **Components are sparse.** A missing field reads as its schema default, so files hold only what someone set,
  and changing a default reaches every entity that never set it.
- **Order keys** are base-62 fractions (`ganbate_scene::order`): inserting between two siblings writes one new key and
  rewrites nothing else. Siblings without one sort after every keyed sibling, by name and then id.
- **Prefabs** are the same document with `"ganbate": "prefab"` and exactly one root. An instance takes the prefab
  root's components beneath its own, shows the prefab's other entities under derived ids, and changes them
  through `overrides` (field-level merges, renames, removals). Instances nest up to 16 deep; a prefab that
  contains itself is refused. Entities can be added under an instance's derived entities.
- **Large worlds** split into layers and cells: a scene may list sub-scenes it includes, and the open-world
  format keeps one file per cell (and optionally one file per entity, like one-file-per-actor) so that teams
  editing different areas never touch the same file. This arrives with streaming (roadmap M9); the version-1
  format already keys everything by id so the split needs no rewrite.

### Component schema

A schema file declares component kinds; the editor draws inspectors from it and the cook refuses data that breaks
it. Engine kinds have no dot (`transform`, `mesh`, `light`, `camera`, `environment`); a game's kinds carry its
namespace (`arena.spawn`).

```json
{
  "ganbate": "schema",
  "components": {
    "arena.spawn": {
      "doc": "Where a player enters.",
      "fields": {
        "facing": {
          "max": 180,
          "min": -180,
          "type": "float",
          "unit": "deg"
        },
        "marker": {
          "kind": "prefab",
          "type": "asset"
        },
        "team": {
          "type": "enum",
          "values": [
            "red",
            "blue"
          ]
        }
      },
      "label": "Spawn point"
    }
  },
  "version": 1
}
```

Field types: `bool`, `int`, `float` (`min`, `max`, `step`, `unit`), `string`, `enum`, `vec2`, `vec3`, `vec4`,
`quat`, `rgb`, `rgba` (linear), `asset` (with the accepted `kind`), `entity`, `array` (`items`, `max`) and `object`
(`fields`). Any field may add `label`, `labels` (per language, for example `{"ko": "시야각"}`), `doc`, `default`,
`hidden`, `readonly`; editors may read further hints
(`slider`, `widget`). A kind's schema changes compatibly by adding fields with defaults; a breaking change bumps a
kind version and ships a migration with the game's editor plugin.

### Material, Look, sequence and the rest

| Document | Holds | Status |
| --- | --- | --- |
| Look (`*.look.json`) | a game's whole style: shading, post-processing, sky palette, place and time, weather, named views, per-material detail rules (fields documented in `src/render/look.rs`) | exists, older than the document rules: in M2 it gains the `ganbate`/`version` header and a sidecar id; readers keep accepting header-less files |
| Variant (`*.variant.json`) | a named set of material and prefab overrides (a table theme, a card back, an outfit) applied at run time | M3 |
| Platform profile (in the project manifest) | per-platform budgets: frame time, draws, memory, download size, which the cook and the device gates check | M4 |
| Material (`*.material.json`) | shading model, factors, textures by id, alpha, detail preset, prop shader and its parameters | M2 |
| Sequence (`*.sequence.json`) | tracks over time: transforms, camera cuts, animation clips, cues (sound, particles, game events), with keys and curves | M3 |
| Animation graph (`*.animgraph.json`) | states, transitions, blend spaces, layers, IK and look-at targets | M6 |
| Particle effect (`*.vfx.json`) | emitters, spawn, forces, curves, sprites or meshes | M7 |
| Sound bank (`*.sounds.json`) | sounds, variations, buses, mix snapshots, cue names | M7 |
| Character (`*.character.json`) | base body, parts, morph values, colours, outfit slots | M6 |
| Terrain (`*.terrain.json` + height/weight images) | heightfield tiles, layers, foliage rules | M5 |

Each follows the same rules: canonical JSON, an `id`, references by id, a version.

## 5. Derived data and the cook

**Import cache (planned with the incremental cook).** Imports (GLB parsing and splitting, image decoding) will be
cached in `.ganbate/cache/` under the SHA-256 of the source bytes, the import settings and the importer's version, so a
clean cache rebuilds identically. In M1 every cook re-imports each model.

**Cook.** In M1, `ganbate-build cook-scene PROJECT SCENE out.gbew [--compact] [--keep-images]` cooks one scene into one
GBEW v4 file, and the editor cooks previews into `.ganbate/cooked/<scene id>.<first 16 hex of sha256>.gbew`. Planned
(M2): `ganbate-build cook-project` (and a Build command in the editor) turns a project into one package per platform:

```text
build/web/
  package.json                      { "assets": { "<asset id>": { "file": "...", "sha256": "...", "bytes": n, "deps": [...] } } }
  8e1f4b2c...-harbour.gbew          a cooked scene: world plus its entity table
  7d0e....glb                       a run-time model (skinned, animated) as the loader reads it
```

A cooked scene is an GBEW world (v4) that keeps authored identity:

- meshes, instances, materials, textures and lamps as in v3;
- an **entity table**: for each entity its id, name, parent, world transform and the contiguous run of instances and
  lamps it produced; plus, as one canonical JSON text, the scene's id and name and the components the cook does not
  bake: game components, cameras, the environment, physics bodies and colliders, meshes with `static: false`, and
  lights that do not follow the lamps (spots and always-on points), which the game places at run time. The cook keeps every entity's geometry separate. Merging for speed is the renderer's job at load time
  (draws grouped by material), and HLOD proxies are a cook *output* beside the entities, never a replacement for
  them.

Cooking is deterministic: the same project, settings and engine version give the same bytes.

**Platforms.** Settings in the manifest choose per platform: texture formats (browser-decoded PNG/JPEG/WebP for
the web today; block-compressed formats when the GPU layer gains them), compact geometry coding, quality tiers.
Console packages are cooked by platform modules kept outside the public repository (see
[runtime](02-runtime.md#platforms)).

## 6. Publication: files never change

A game that serves files to players must never change a published file, because caches, CDNs and old clients
hold it. The engine makes that the default rather than a discipline:

1. (Planned, M2.) `ganbate-build publish` copies each cooked file to
   `publish/<platform>/<name>.<first 16 hex of sha256>.<ext>`.
   A file with that name already present must have the same bytes, or publishing stops.
2. It appends the new entries to `publish/<platform>/published.json` (asset id, file, sha256, bytes, the cook's
   engine revision). Old entries are never edited or removed by the tool.
3. A game's own release manifest points at entries from that list. Rolling back is pointing at older entries.

A game that keeps its own published-assets manifest (file name plus sha256) records the engine's entries in it;
the engine's rule and the game's agree because both forbid rewriting a name.

## 7. Git

- Commit `ganbate.project.json`, `content/`, `schemas/`, `editor/` and, if the game ships from the repository,
  `publish/`. Ignore `.ganbate/` and `build/`. Large binaries may use the game's LFS policy; the engine does not
  require it.
- Canonical text plus id-keyed entities makes most merges automatic. `ganbate-build check` (planned, M2) will validate
  every document (ids, references, schemas, prefab cycles) and be the pre-commit and CI gate.
- A semantic merge driver (`ganbate-build merge`: three-way merge of entity maps by id, field by field) is planned
  for M9; until then, a conflict is ordinary JSON lines.
