// The editor's words in English and Korean. Component and field labels come from schemas: a
// schema may give `"labels": {"ko": "..."}` on a component or a field, and the engine's own
// schema does.

const STRINGS = {
  en: {
    projectTools: 'Project tools', openSeparately: 'Open separately', backToScene: 'Back to scene',
    save: 'Save', saveTip: 'Save (Ctrl+S)',
    undo: 'Undo', undoTip: 'Undo (Ctrl+Z)', undoX: 'Undo {x}', nothingToUndo: 'Nothing to undo',
    redo: 'Redo', redoTip: 'Redo (Ctrl+Shift+Z)', redoX: 'Redo {x}', nothingToRedo: 'Nothing to redo',
    move: 'Move', moveTip: 'Move (W)', rotate: 'Rotate', rotateTip: 'Rotate (E)', scale: 'Scale', scaleTip: 'Scale (R)',
    frame: 'Frame', frameTip: 'Frame the selection (F)', time: 'Time', refresh: 'Refresh', refreshTip: 'Cook the scene again',
    simulate: 'Simulate', simulateTip: 'Run the physics of rigid bodies and colliders; again to stop (P)', stopSimulation: 'Stop',
    noPhysics: 'This scene has no rigid bodies or colliders to simulate',
    assets: 'Assets', newScene: '+ Scene', newSceneTip: 'New scene', outliner: 'Outliner', inspector: 'Inspector',
    console: 'Console', filter: 'Filter', open: 'Open', importAsScene: 'Import as scene', newSceneFile: 'New scene file',
    noDocument: 'No document', addEntityTip: 'Add an entity (under the selection)', duplicateTip: 'Duplicate (Ctrl+D)',
    deleteTip: 'Delete (Del)', selectHint: 'Select an entity in the viewport or the outliner.',
    openHint: 'Open a scene from Assets.', name: 'Name', addComponent: 'Add component…', removeComponent: 'Remove component',
    resetDefault: 'Back to the default', lookThrough: 'Look through', noSchema: 'No schema for {x}.',
    connecting: 'connecting', connected: 'connected', disconnected: 'disconnected', language: '한국어',
    entity: 'Entity', vMove: 'Move', vRotate: 'Rotate', vScale: 'Scale', vRename: 'Rename', vAdd: 'Add',
    vAddEntity: 'Add entity', vDelete: 'Delete', vDuplicate: 'Duplicate', vSet: 'Set', vReset: 'Reset', vRemove: 'Remove',
    copy: 'copy', eulerTip: 'pitch (X), yaw (Y), roll (Z) in degrees', none: '(none)',
    overwriteChanged: 'This file changed on disk after it was opened (another tool, git or editor wrote it). Overwrite it with the version shown here?',
  },
  ko: {
    projectTools: '프로젝트 도구', openSeparately: '새 탭에서 열기', backToScene: '장면으로 돌아가기',
    save: '저장', saveTip: '저장 (Ctrl+S)',
    undo: '실행 취소', undoTip: '실행 취소 (Ctrl+Z)', undoX: '실행 취소: {x}', nothingToUndo: '되돌릴 작업 없음',
    redo: '다시 실행', redoTip: '다시 실행 (Ctrl+Shift+Z)', redoX: '다시 실행: {x}', nothingToRedo: '다시 할 작업 없음',
    move: '이동', moveTip: '이동 (W)', rotate: '회전', rotateTip: '회전 (E)', scale: '크기', scaleTip: '크기 (R)',
    frame: '맞춤', frameTip: '선택 항목에 맞춤 (F)', time: '시각', refresh: '다시 굽기', refreshTip: '씬을 다시 굽기',
    simulate: '시뮬레이션', simulateTip: '강체와 충돌체의 물리를 실행합니다. 다시 누르면 멈춥니다 (P)', stopSimulation: '정지',
    noPhysics: '이 씬에는 시뮬레이션할 강체나 충돌체가 없습니다',
    assets: '에셋', newScene: '+ 씬', newSceneTip: '새 씬', outliner: '아웃라이너', inspector: '인스펙터',
    console: '콘솔', filter: '검색', open: '열기', importAsScene: '씬으로 가져오기', newSceneFile: '새 씬 파일',
    noDocument: '문서 없음', addEntityTip: '엔티티 추가 (선택 항목 아래)', duplicateTip: '복제 (Ctrl+D)',
    deleteTip: '삭제 (Del)', selectHint: '뷰포트나 아웃라이너에서 엔티티를 선택하세요.',
    openHint: '에셋에서 씬을 여세요.', name: '이름', addComponent: '컴포넌트 추가…', removeComponent: '컴포넌트 제거',
    resetDefault: '기본값으로', lookThrough: '이 카메라로 보기', noSchema: '{x}의 스키마가 없습니다.',
    connecting: '연결 중', connected: '연결됨', disconnected: '연결 끊김', language: 'English',
    entity: '엔티티', vMove: '이동', vRotate: '회전', vScale: '크기 조정', vRename: '이름 변경', vAdd: '추가',
    vAddEntity: '엔티티 추가', vDelete: '삭제', vDuplicate: '복제', vSet: '설정', vReset: '초기화', vRemove: '제거',
    copy: '복사본', eulerTip: '피치(X), 요(Y), 롤(Z), 도 단위', none: '(없음)',
    overwriteChanged: '이 파일은 연 뒤에 디스크에서 바뀌었습니다(다른 도구, git 또는 다른 편집기가 저장). 여기 보이는 버전으로 덮어쓸까요?',
  },
};

function initial() {
  try {
    const saved = localStorage.getItem('abe.studio.lang');
    if (saved && STRINGS[saved]) return saved;
  } catch { /* storage may be unavailable */ }
  return (navigator.language || 'en').toLowerCase().startsWith('ko') ? 'ko' : 'en';
}

let current = initial();

export const lang = () => current;

export function setLang(l) {
  current = STRINGS[l] ? l : 'en';
  try { localStorage.setItem('abe.studio.lang', current); } catch { /* per-viewer convenience only */ }
}

export function t(key, x) {
  const s = STRINGS[current][key] ?? STRINGS.en[key] ?? key;
  return x === undefined ? s : s.replace('{x}', x);
}

/** A schema label in the current language: `labels[lang]`, then `label`, then the name. */
export function label(def, name) {
  return def?.labels?.[current] || def?.label || name;
}

/** Applies `data-i18n` (text) and `data-i18n-title` (tooltip) attributes under `root`. */
export function translate(root = document) {
  for (const e of root.querySelectorAll('[data-i18n]')) e.textContent = t(e.dataset.i18n);
  for (const e of root.querySelectorAll('[data-i18n-title]')) e.title = t(e.dataset.i18nTitle);
  for (const e of root.querySelectorAll('[data-i18n-placeholder]')) e.placeholder = t(e.dataset.i18nPlaceholder);
  document.documentElement.lang = current;
}
