import test from 'node:test';
import assert from 'node:assert/strict';
import {parseObj,perspective,lookAt,orbitEye} from './viewer.js';

const quad='# wing panel\nv 0 0 0\nv 2 0 0\nv 2 4 0\nv 0 4 0\ng MeshGeom\nf 1 2 3 4\n';

test('faces fan into flat-shaded triangles with vehicle extents',()=>{
  const mesh=parseObj(quad);
  assert.equal(mesh.triangles,2);
  assert.equal(mesh.vertices,4);
  assert.deepEqual(mesh.minimum,[0,0,0]);
  assert.deepEqual(mesh.maximum,[2,4,0]);
  assert.equal(mesh.positions.length,18);
  assert.deepEqual([...mesh.normals.slice(0,3)],[0,0,1]);
  assert.deepEqual([...mesh.positions.slice(9,18)],[0,0,0,2,4,0,0,4,0]);
});

test('index forms with slashes and negative references resolve to the same vertices',()=>{
  const slashed=parseObj('v 0 0 0\nv 2 0 0\nv 2 4 0\nf 1/1/1 2/2/2 3/3/3\n');
  const negative=parseObj('v 0 0 0\nv 2 0 0\nv 2 4 0\nf -3 -2 -1\n');
  assert.deepEqual([...slashed.positions],[...negative.positions]);
});

test('degenerate or malformed files are rejected',()=>{
  assert.throws(()=>parseObj('v 0 0 0\n'),/no triangles/);
  assert.throws(()=>parseObj('v 0 0 0\nv 1 0 0\nf 1 2 9\n'),/missing vertex/);
  assert.throws(()=>parseObj('v 0 0 nan\nv 1 0 0\nv 0 1 0\nf 1 2 3\n'),/non-finite/);
});

test('camera helpers keep z up and frame the model centre',()=>{
  const eye=orbitEye([10,0,2],8,0,0);
  assert.deepEqual(eye,[18,0,2]);
  const view=lookAt(eye,[10,0,2],[0,0,1]);
  const depth=view[2]*10+view[6]*0+view[10]*2+view[14];
  assert.ok(Math.abs(depth+8)<1e-12,'the centre sits eight units down the view axis');
  assert.ok(view[9]>0&&Math.abs(view[8])<1e-12,'vehicle z maps to screen up with no sideways component');
  const projection=perspective(0.6,2,0.1,100);
  assert.ok(projection[0]>0&&projection[5]>0&&projection[11]===-1);
});
