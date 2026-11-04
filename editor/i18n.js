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
    assets: 'Assets', newScene: '+ Scene', newSceneTip: 'New scene', newPrefab: '+ Prefab', newPrefabTip: 'New prefab', newPrefabFile: 'New prefab file',
    searchAssets: 'Search assets…', allFolders: 'All Assets', allKinds: 'All',
    kind_scene: 'Scene', kind_prefab: 'Prefab', kind_model: 'Model', kind_look: 'Look', kind_data: 'Data',
    kind_material: 'Material', kind_texture: 'Texture', kind_sound: 'Sound',
    kind_soundbank: 'Soundbank', kind_character: 'Character', kind_seq: 'Sequence', kind_vfx: 'VFX',
    editMaterial: 'Edit Material', openModelStudio: 'Model Studio',
    newData: '+ Data', newDataTip: 'New schema-validated game data document', newDataTitle: 'New Data Document',
    dataSchema: 'Schema', dataPath: 'File path', createDoc: 'Create', cancel: 'Cancel', close: 'Close',
    editData: 'Edit Data', dataLoading: 'Opening data document…',
    noGameSchemas: 'No game schemas (*.schema.json) are defined in this project yet.',
    noDataSchemas: 'No game schemas (*.schema.json) are defined in this project yet.',
    addItem: '+ Add item', moveUp: 'Move up', moveDown: 'Move down',
    viewScene: 'Scene', viewPie: 'Game (PIE)', viewMatrix: 'Device Matrix',
    pieResponsive: 'Responsive · Fit', pieInspectUi: 'Inspect UI', pieInspectUiTip: 'Pick game UI elements to inspect their data fields',
    layoutRig: 'Layout Rig', layoutRigTip: 'Toggle interactive 2D anchor handles', mirrorX: 'Mirror X', snapShiftHint: 'Shift: snap',
    sequencer: 'Sequencer', sequencerTip: 'Open Sequencer Workbench',
    liveRelease: 'Live Release', liveLocal: 'Local', liveDraft: 'Draft', liveRev: 'Live r{x}',
    liveLocalHint: 'No live release published yet', liveDraftReady: 'Draft staged on game server', livePublishedAt: 'Published live revision r{x}',
    liveNotePlaceholder: 'Release note (optional)…',
    liveRefresh: 'Refresh Status', liveStage: 'Stage Draft', livePublish: 'Publish Live (Skip Deploy)',
    liveDiscard: 'Discard Draft', livePull: 'Pull Live to Project',
    liveStagedLog: 'Staged {x} documents to draft', livePublishedLog: 'Published live release r{x}',
    liveDiscardedLog: 'Discarded staged draft', livePulledLog: 'Pulled {x} changed documents from live server',
    wb_world: '3D World & Look', wb_model: 'Model & Mesh', wb_material: 'Material Lab',
    wb_character: 'Character Studio', wb_sequencer: 'Sequencer', wb_vfx: 'VFX Lab',
    wb_audio: 'Sound & Foley DAW', wb_layout_rig: '2D/3D Layout Rig', wb_profiler: 'Telemetry Profiler',
    wb_code_xray: 'Project X-Ray',
    wbUnavailableTitle: 'Workbench Ready for Mount',
    wbUnavailableHint: 'Module {x} is not bundled in this standalone build yet; it mounts automatically once merged.',
    aiSelectionNone: '@selection: scene',
    aiGbsPlaceholder: '.gbs command (e.g. spawn Beacon at 0 1 0; snap Beacon ground)…',
    aiApply: 'Apply .gbs', aiApplyTip: 'Execute Ganbate SceneScript (.gbs) as one undoable Studio transaction',
    aiVerify: 'Verify', aiVerifyTip: 'Run <100ms structural, spatial & offscreen silhouette verification',
    aiCatalog: 'Catalog', aiCatalogTip: 'Query semantic asset & brick catalog',
    aiCrystallize: 'Crystallize', aiCrystallizeTip: 'Crystallize current selection into a reusable prefab brick',
    aiCrystallizePrompt: 'Name for the reusable prefab brick:',
    gridView: 'Grid', gridViewTip: 'Show assets as thumbnail cards', listView: 'List', listViewTip: 'Show assets as a detailed list',
    placeInScene: 'Place', placeInSceneTip: 'Place into the open scene (or drag into the viewport)',
    openSceneToPlace: 'Open a scene or prefab first to place assets', cannotPlaceSelf: 'A prefab cannot be placed inside itself',
    dropToPlace: 'Drop to place {x}', dropToAssign: 'Drop to assign {x}',
    noMatchingAssets: 'No assets match the current filter.', noProjectAssets: 'No assets in this project yet.', clearFilters: 'Clear filters',
    copyId: 'Copy asset ID', copiedId: 'Copied', copyPath: 'Copy path', previewSound: 'Play', stopSound: 'Stop', toggleConsole: 'Toggle Console',
    importAssets: '+ Import', importAssetsTip: 'Import asset files from disk (.glb, .png, .jpg, .webp, .wav, .ogg, .json)',
    dropFilesToImport: 'Drop files to import into {x}', importedCount: 'Imported {x}',
    favorites: 'Favorites', recentAssets: 'Recent', unusedAssets: 'Unused', foldersHeading: 'Folders',
    favoriteAsset: 'Favorite', unfavoriteAsset: 'Unfavorite',
    sortKind: 'Sort: Kind', sortName: 'Sort: Name', sortFolder: 'Sort: Folder', sortSize: 'Sort: Size',
    cardScaleTip: 'Thumbnail size',
    assetDetails: 'Details', toggleAssetDetails: 'Toggle Asset Details & References pane',
    selectAssetHint: 'Select an asset to inspect its preview, metadata, and references.',
    renameAsset: 'Rename…', renameAssetPrompt: 'New name for {x}', moveAsset: 'Move…', moveAssetPrompt: 'Target path for {x}',
    whereUsed: 'Where Used', unusedBadge: 'Unused', usedInCount: '{x} references', noReferences: 'Not referenced by any scene, prefab, or material.',
    startupSceneRef: 'Project startup scene', loadingReferences: 'Scanning references…',
    modelStats: '{v} verts · {t} tris', boundsMeters: '{x} m',
    vPlace: 'Place', outliner: 'Outliner', inspector: 'Inspector',
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
    assets: '에셋', newScene: '+ 씬', newSceneTip: '새 씬', newPrefab: '+ 프리팹', newPrefabTip: '새 프리팹', newPrefabFile: '새 프리팹 파일',
    searchAssets: '에셋 검색…', allFolders: '전체 에셋', allKinds: '전체',
    kind_scene: '씬', kind_prefab: '프리팹', kind_model: '모델', kind_look: '룩', kind_data: '데이터',
    kind_material: '재질', kind_texture: '텍스처', kind_sound: '사운드',
    kind_soundbank: '사운드뱅크', kind_character: '캐릭터', kind_seq: '시퀀스', kind_vfx: '이펙트',
    editMaterial: '재질 편집', openModelStudio: '모델 스튜디오',
    newData: '+ 데이터', newDataTip: '새 스키마 검증 게임 데이터 문서', newDataTitle: '새 데이터 문서',
    dataSchema: '스키마', dataPath: '파일 경로', createDoc: '생성', cancel: '취소', close: '닫기',
    editData: '데이터 편집', dataLoading: '데이터 문서를 여는 중…',
    noGameSchemas: '프로젝트에 정의된 게임 스키마(*.schema.json)가 없습니다.',
    noDataSchemas: '프로젝트에 정의된 게임 스키마(*.schema.json)가 없습니다.',
    addItem: '+ 항목 추가', moveUp: '위로 이동', moveDown: '아래로 이동',
    viewScene: '씬', viewPie: '게임 (PIE)', viewMatrix: '디바이스 매트릭스',
    pieResponsive: '반응형 · 맞춤', pieInspectUi: 'UI 검사', pieInspectUiTip: '게임 UI 요소를 클릭하여 데이터 필드로 이동',
    layoutRig: '레이아웃 리그', layoutRigTip: '반응형 2D 앵커 핸들 표시 전환', mirrorX: '좌우 대칭', snapShiftHint: 'Shift: 스냅',
    sequencer: '시퀀서', sequencerTip: '시퀀서 워크벤치 열기',
    liveRelease: '라이브 배포', liveLocal: '로컬', liveDraft: '초안', liveRev: '라이브 r{x}',
    liveLocalHint: '배포된 라이브 리비전 없음', liveDraftReady: '게임 서버에 초안 스테이징됨', livePublishedAt: '라이브 리비전 r{x} 배포됨',
    liveNotePlaceholder: '배포 메모 (선택)…',
    liveRefresh: '상태 새로고침', liveStage: '초안 스테이징', livePublish: '라이브 즉시 배포 (빌드 생략)',
    liveDiscard: '초안 폐기', livePull: '라이브 변경사항 가져오기',
    liveStagedLog: '문서 {x}개를 초안으로 스테이징함', livePublishedLog: '라이브 릴리스 r{x} 배포 완료',
    liveDiscardedLog: '스테이징된 초안을 폐기함', livePulledLog: '라이브 서버에서 변경된 문서 {x}개를 가져옴',
    wb_world: '3D 월드 · 룩', wb_model: '모델 · 메시', wb_material: '재질 랩',
    wb_character: '캐릭터 스튜디오', wb_sequencer: '시퀀서', wb_vfx: 'VFX 랩',
    wb_audio: '사운드 · 폴리 DAW', wb_layout_rig: '2D/3D 레이아웃 리그', wb_profiler: '텔레메트리 프로파일러',
    wb_code_xray: '프로젝트 X-Ray',
    wbUnavailableTitle: '워크벤치 마운트 대기 중',
    wbUnavailableHint: '{x} 모듈은 병합 후 자동으로 마운트됩니다.',
    aiSelectionNone: '@selection: 씬',
    aiGbsPlaceholder: '.gbs 명령 입력 (예: spawn Beacon at 0 1 0; snap Beacon ground)…',
    aiApply: '.gbs 적용', aiApplyTip: 'Ganbate SceneScript (.gbs)를 실행 취소 가능한 단일 트랜잭션으로 적용',
    aiVerify: '검증', aiVerifyTip: '100ms 미만의 구조 · 공간 · 실루엣 오프스크린 검증 실행',
    aiCatalog: '카탈로그', aiCatalogTip: '시맨틱 에셋 · 브릭 카탈로그 조회',
    aiCrystallize: '브릭화', aiCrystallizeTip: '선택한 엔티티를 재사용 가능한 프리팹 브릭으로 저장',
    aiCrystallizePrompt: '프리팹 브릭 이름:',
    gridView: '격자', gridViewTip: '썸네일 카드로 보기', listView: '목록', listViewTip: '상세 목록으로 보기',
    placeInScene: '배치', placeInSceneTip: '열린 씬에 배치 (또는 뷰포트로 드래그)',
    openSceneToPlace: '에셋을 배치하려면 먼저 씬이나 프리팹을 여세요', cannotPlaceSelf: '프리팹은 자기 자신 안에 배치할 수 없습니다',
    dropToPlace: '여기에 놓아 {x} 배치', dropToAssign: '여기에 놓아 {x} 지정',
    noMatchingAssets: '조건에 맞는 에셋이 없습니다.', noProjectAssets: '이 프로젝트에는 아직 에셋이 없습니다.', clearFilters: '필터 지우기',
    copyId: '에셋 ID 복사', copiedId: '복사됨', copyPath: '경로 복사', previewSound: '재생', stopSound: '정지', toggleConsole: '콘솔 접기/펼치기',
    importAssets: '+ 가져오기', importAssetsTip: '디스크에서 에셋 파일 가져오기 (.glb, .png, .jpg, .webp, .wav, .ogg, .json)',
    dropFilesToImport: '{x} 폴더로 파일 가져오기', importedCount: '{x} 가져옴',
    favorites: '즐겨찾기', recentAssets: '최근 항목', unusedAssets: '미사용', foldersHeading: '폴더',
    favoriteAsset: '즐겨찾기 추가', unfavoriteAsset: '즐겨찾기 해제',
    sortKind: '정렬: 종류', sortName: '정렬: 이름', sortFolder: '정렬: 폴더', sortSize: '정렬: 크기',
    cardScaleTip: '썸네일 크기',
    assetDetails: '상세', toggleAssetDetails: '에셋 상세 및 참조 패널 열기/닫기',
    selectAssetHint: '미리보기, 메타데이터, 참조 목록을 보려면 에셋을 선택하세요.',
    renameAsset: '이름 변경…', renameAssetPrompt: '{x}의 새 이름', moveAsset: '이동…', moveAssetPrompt: '{x}의 이동 경로',
    whereUsed: '사용처', unusedBadge: '미사용', usedInCount: '참조 {x}곳', noReferences: '씬, 프리팹, 재질에서 참조되지 않습니다.',
    startupSceneRef: '프로젝트 시작 씬', loadingReferences: '참조 확인 중…',
    modelStats: '정점 {v} · 삼각형 {t}', boundsMeters: '{x} m',
    vPlace: '배치', outliner: '아웃라이너', inspector: '인스펙터',
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
    const saved = localStorage.getItem('ganbate.studio.lang');
    if (saved && STRINGS[saved]) return saved;
  } catch { /* storage may be unavailable */ }
  return (navigator.language || 'en').toLowerCase().startsWith('ko') ? 'ko' : 'en';
}

let current = initial();

export const lang = () => current;

export function setLang(l) {
  current = STRINGS[l] ? l : 'en';
  try { localStorage.setItem('ganbate.studio.lang', current); } catch { /* per-viewer convenience only */ }
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
