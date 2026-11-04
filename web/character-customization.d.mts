/** Minimal Ganbate model interface. The native engine owns the model, its render data and its pose. */
export function bindCharacterShapes<K extends string>(
  engine: {
    modelMorph(instance: number, node: number, target: number, weight: number): void;
    updateModel(instance: number): void;
  },
  instance: number,
  info: {
    readonly nodes: readonly { readonly mesh: number | null }[];
    readonly meshes: readonly { readonly targetNames: readonly string[]; readonly targets: number }[];
  },
  ranges: Readonly<Record<K, readonly [number, number]>>,
): {
  readonly available: readonly K[];
  /** Returns true if new weights were committed. Force reapplies weights after external animation. */
  apply(weights: Readonly<Partial<Record<K, number>>>, force?: boolean): boolean;
};
