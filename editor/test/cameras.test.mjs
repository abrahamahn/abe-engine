import assert from 'node:assert/strict';
import test from 'node:test';
import { cameraWorld, captureCamera, orbitBasis, orbitFromCamera } from '../cameras.js';
import { fromTRS, IDENTITY, multiply, quatFromEuler } from '../math.js';

function close(actual, expected) {
  actual.forEach((value, i) => assert.ok(Math.abs(value - expected[i]) < 1e-7,
    `value ${i}: ${value} should match ${expected[i]}`));
}
function eye(view) {
  return view.target.map((v, i) => v + view.distance * orbitBasis(view).back[i]);
}

test('capture and look through keep the camera position, direction, roll and lens', () => {
  const parent = multiply(fromTRS([4, 8, -3], quatFromEuler([28, -22, 11]), [-1.5, 3, 2]),
    fromTRS([0, 2, 1], quatFromEuler([0, 30, 14]), [1, 2, 3]));
  for (const pitch of [0.76, Math.PI/2, -Math.PI/2]) {
    const view = { yaw: -0.46, pitch, roll: 1.7, target: [6, 0.5, -2], distance: 9, fov: 61 };
    const existing = { scale: [1, -1.2, -0.8], extension: 'kept' };
    const local = captureCamera(view, parent, existing);
    assert.deepEqual(local.scale, existing.scale);
    assert.equal(local.extension, 'kept');
    const camera = multiply(parent, fromTRS(local.translation, local.rotation, local.scale));
    const result = orbitFromCamera(camera, view.fov);
    close(eye(result), eye(view));
    close(orbitBasis(result).back, orbitBasis(view).back);
    close(orbitBasis(result).up, orbitBasis(view).up);
    assert.equal(result.fov, 61);
  }
});

test('uses authored parents with their current transforms, and refuses collapsed or cyclic paths', () => {
  const entities = {
    pivot: { components: { transform: { translation: [1, 2, 3] } } },
    socket: { parent: 'pivot', components: { transform: { translation: [0, 2, 0] } } },
  };
  close(cameraWorld(entities, 'socket').slice(12, 15), [1, 4, 3]);
  assert.equal(cameraWorld(entities, 'missing'), null);
  entities.pivot.parent = 'socket';
  assert.equal(cameraWorld(entities, 'socket'), null);
  assert.deepEqual(cameraWorld(entities, undefined), IDENTITY);
  const flat = fromTRS([0, 0, 0], [0, 0, 0, 1], [1, 1, 0]);
  assert.equal(orbitFromCamera(flat), null);
  assert.equal(captureCamera({}, flat), null);
  assert.equal(captureCamera({}, IDENTITY, { scale: [1, 0, 1] }), null);
});
