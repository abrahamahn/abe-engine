/** Bind named glTF morphs to one Ganbate model instance. Sharing a model does not share its pose.
 * Pass only the channels this editor owns. Omitted weights leave the current pose alone.
 * Call apply with force after another producer (for example an animation clip) writes the pose.
 */
export function bindCharacterShapes(engine, instance, info, ranges) {
  const bindings = new Map();
  for (const [node, description] of info.nodes.entries()) {
    if (description.mesh == null) continue;
    const mesh = info.meshes[description.mesh];
    if (!mesh) continue;
    for (const [target, name] of mesh.targetNames.entries()) {
      if (!Object.hasOwn(ranges, name) || target >= mesh.targets) continue;
      const [min, max] = ranges[name];
      if (!Number.isFinite(min) || !Number.isFinite(max) || min > max) {
        throw new RangeError('Invalid morph range for ' + name);
      }
      let group = bindings.get(name);
      if (!group) bindings.set(name, group = { min, max, slots: [] });
      group.slots.push([node, target]);
    }
  }
  const applied = new Map();
  return {
    available: Object.freeze([...bindings.keys()]),
    apply(weights, force = false) {
      const pending = [];
      for (const [name, raw] of Object.entries(weights)) {
        const group = bindings.get(name);
        if (!group) continue;
        if (!Number.isFinite(raw)) throw new TypeError('Invalid morph weight for ' + name);
        const value = Math.min(group.max, Math.max(group.min, raw));
        if (force || value !== applied.get(name)) pending.push({ name, group, value });
      }
      if (!pending.length) return false;
      for (const { group, value } of pending) {
        for (const [node, target] of group.slots) engine.modelMorph(instance, node, target, value);
      }
      engine.updateModel(instance);
      for (const { name, value } of pending) applied.set(name, value);
      return true;
    },
  };
}
