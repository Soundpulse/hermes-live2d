import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const modelDir = resolve(root, "public/model");
const manifest = JSON.parse(
  readFileSync(resolve(modelDir, "Sparky.model3.json"), "utf8"),
);

test("the Sparky manifest only references files in the bundled model", () => {
  const refs = manifest.FileReferences;
  const paths = [
    refs.Moc,
    ...refs.Textures,
    refs.Physics,
    refs.DisplayInfo,
    ...Object.values(refs.Motions).flatMap((motions) =>
      motions.map((motion) => motion.File),
    ),
    ...refs.Expressions.map((expression) => expression.File),
  ];

  for (const path of paths) {
    assert.equal(existsSync(resolve(modelDir, path)), true, path);
  }
});

test("expression IDs retain the public six-emotion order and parameters", () => {
  const expected = [
    ["normal", 0, 0],
    ["heart", 1, 0],
    ["blank", 2, 0],
    ["angry", 2, 1],
    ["dizzy", 3, 0],
    ["><", 4, 0],
  ];

  const actual = manifest.FileReferences.Expressions.map((expression) => {
    const preset = JSON.parse(
      readFileSync(resolve(modelDir, expression.File), "utf8"),
    );
    const values = Object.fromEntries(
      preset.Parameters.map((parameter) => [parameter.Id, parameter.Value]),
    );
    return [expression.Name, values.ParamEyeStyle, values.ParamAnger];
  });

  assert.deepEqual(actual, expected);
});

test("cursor tracking uses Live2D focus instead of racing motion parameters", () => {
  const source = readFileSync(resolve(root, "src/main.ts"), "utf8");

  assert.match(
    source,
    /focusController\.focus\(smoothedFocusX, smoothedFocusY\)/,
  );
  assert.doesNotMatch(
    source,
    /setParameterValueById\("Param(?:Angle|EyeBall|BodyAngle)/,
  );
});

test("cursor tracking caps the final head tilt without limiting the eyes", () => {
  const source = readFileSync(resolve(root, "src/main.ts"), "utf8");

  assert.match(source, /const MAX_HEAD_TILT_DEGREES = 9;/);
  assert.match(source, /internalModel\.on\("beforeModelUpdate"/);
  for (const parameter of ["ParamAngleX", "ParamAngleY", "ParamAngleZ"]) {
    assert.match(source, new RegExp(`clampModelParameter\\(coreModel, "${parameter}"`));
  }
});

test("cursor direction pivots around the model eye line", () => {
  const source = readFileSync(resolve(root, "src/main.ts"), "utf8");

  assert.match(source, /const FOCUS_EYE_LINE = 0\.3;/);
  assert.match(source, /const modelCenterX = currentModel\.x;/);
  assert.match(
    source,
    /const modelEyeY = currentModel\.y - currentModel\.height \/ 2 \+\s*currentModel\.height \* FOCUS_EYE_LINE;/,
  );
  assert.match(source, /const dy = cy - modelEyeY;/);
});

test("rapid cursor changes are softened and upward iris travel is capped", () => {
  const source = readFileSync(resolve(root, "src/main.ts"), "utf8");

  assert.match(source, /const FOCUS_SMOOTHING = 0\.12;/);
  assert.match(source, /const MAX_IRIS_UP = 0\.5;/);
  assert.match(
    source,
    /smoothedFocusX \+= \(focusTargetX - smoothedFocusX\) \* FOCUS_SMOOTHING;/,
  );
  assert.match(
    source,
    /smoothedFocusY \+= \(focusTargetY - smoothedFocusY\) \* FOCUS_SMOOTHING;/,
  );
  assert.match(source, /Math\.min\(MAX_IRIS_UP, irisY\)/);
});

test("hover includes the full hair area while keeping other edges tight", () => {
  const source = readFileSync(resolve(root, "src/main.ts"), "utf8");

  assert.match(source, /const hoverTop = currentModel\.y - currentModel\.height \/ 2;/);
  assert.match(
    source,
    /const hoverBottom = currentModel\.y \+ \(currentModel\.height \/ 2\) \* HOVER_HITBOX;/,
  );
  assert.match(source, /cy >= hoverTop && cy <= hoverBottom/);
  assert.doesNotMatch(source, /cy >= currentModel\.y - halfH/);
});

test("the model is rendered five percent smaller", () => {
  const source = readFileSync(resolve(root, "src/main.ts"), "utf8");

  assert.match(source, /const MODEL_FIT = 0\.57;/);
});
