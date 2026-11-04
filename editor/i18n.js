// The editor's words in English and Korean. Component and field labels come from schemas: a
// schema may give `"labels": {"ko": "..."}` on a component or a field, and the engine's own
// schema does.

const STRINGS = {
  en: {
    rescanAssets: 'Rescan', rescanAssetsTip: 'Scan the project content folders again', scanFailed: 'Rescan failed. The last asset list is still in use.',
    scanWorking: 'Scanning project content…', scanComplete: '{x} assets', scanAdded: 'Added', scanMoved: 'Moved', scanRemoved: 'Removed', scanIdentity: 'New sidecar',
    scanProblems: 'Problems', sourceUnavailable: 'Source unavailable', sourceChanged: 'File changed on disk', missingDrafts: 'Unavailable drafts', downloadDraft: 'Download draft',
    newLook: '+ Look', newLookFile: 'New Look file', missingLook: 'The referenced Look is missing from the project',
    lookSceneTime: 'The scene environment sets the preview time to {x} h.', lookSceneWeather: 'The scene environment sets the preview weather preset to {x}.',
    lookSection: 'Look section', lookDefault: 'Engine default', lookLoading: 'Opening Look…', lookRetry: 'Retry', editLook: 'Edit Look',
    look_shading: 'Shading', look_post: 'Post-processing', look_sky: 'Sky', look_time: 'Time and place',
    look_weather: 'Weather', look_grass: 'Grass', look_terrain: 'Terrain', look_details: 'Material detail rules', look_views: 'Named views',
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
    resetDefault: 'Back to the default', lookThrough: 'Look through',
    frameOptions: 'View guides', previewShape: 'Preview shape', availableSpace: 'Available space',
    portraitFrame: 'Portrait · 9:16', landscapeFrame: 'Landscape · 16:9', classicFrame: 'Classic · 4:3',
    marginGuide: 'Margin guide', guideInset: 'Guide inset', viewLens: 'Lens', viewLensTip: 'Vertical field of view in degrees', alignCamera: 'Align camera to view', bookmarkCamera: '+ Camera', bookmarkCameraTip: 'Save this view as a named camera', cameraName: 'Name this camera', camera: 'Camera', cameraSingular: 'The camera or its parent has a collapsed transform.',
    noSchema: 'No schema for {x}.',
    connecting: 'connecting', connected: 'connected', disconnected: 'disconnected', language: '한국어',
    entity: 'Entity', vMove: 'Move', vRotate: 'Rotate', vScale: 'Scale', vRename: 'Rename', vAdd: 'Add',
    vAddEntity: 'Add entity', vDelete: 'Delete', vDuplicate: 'Duplicate', vSet: 'Set', vReset: 'Reset', vRemove: 'Remove',
    copy: 'copy', eulerTip: 'pitch (X), yaw (Y), roll (Z) in degrees', none: '(none)',
    overwriteChanged: 'This file changed on disk after it was opened (another tool, git or editor wrote it). Overwrite it with the version shown here?',
  },
  ko: {
    rescanAssets: '다시 검색', rescanAssetsTip: '프로젝트 콘텐츠 폴더 다시 검색', scanFailed: '검색하지 못했습니다. 이전 목록을 계속 사용합니다.',
    scanWorking: '프로젝트 콘텐츠 검색 중…', scanComplete: '에셋 {x}개', scanAdded: '추가', scanMoved: '이동', scanRemoved: '삭제', scanIdentity: '새 사이드카',
    scanProblems: '문제', sourceUnavailable: '원본을 사용할 수 없음', sourceChanged: '디스크 파일이 변경됨', missingDrafts: '원본 없는 초안', downloadDraft: '초안 다운로드',
    newLook: '+ 룩', newLookFile: '새 룩 파일', missingLook: '프로젝트에서 지정된 룩을 찾을 수 없습니다',
    lookSceneTime: '씬 환경이 미리보기 시각을 {x}시간으로 설정합니다.', lookSceneWeather: '씬 환경이 미리보기 날씨 프리셋을 {x}(으)로 설정합니다.',
    lookSection: '룩 항목', lookDefault: '엔진 기본값', lookLoading: '룩을 여는 중…', lookRetry: '다시 시도', editLook: '룩 편집',
    look_shading: '셰이딩', look_post: '후처리', look_sky: '하늘', look_time: '시간과 장소',
    look_weather: '날씨', look_grass: '풀', look_terrain: '지형', look_details: '재질 디테일 규칙', look_views: '이름 있는 뷰',
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
    resetDefault: '기본값으로', lookThrough: '이 카메라로 보기',
    frameOptions: '화면 가이드', previewShape: '미리보기 비율', availableSpace: '가용 공간',
    portraitFrame: '세로 · 9:16', landscapeFrame: '가로 · 16:9', classicFrame: '기본 · 4:3',
    marginGuide: '여백 가이드', guideInset: '가이드 여백', viewLens: '렌즈', viewLensTip: '세로 시야각(도)', alignCamera: '카메라를 현재 시점에 맞추기', bookmarkCamera: '+ 카메라', bookmarkCameraTip: '현재 시점을 이름 있는 카메라로 저장', cameraName: '카메라 이름', camera: '카메라', cameraSingular: '카메라 또는 상위 개체의 변환이 축소되어 있습니다.',
    noSchema: '{x}의 스키마가 없습니다.',
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
