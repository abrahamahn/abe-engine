// The frame changes the actual render canvas, so picks, handles and screenshots use its aspect.
// Margin guides are composition aids; they don't change the scene or assume a device's cutouts.
export function setupViewportFrame() {
  const frame = document.querySelector('#preview-frame');
  const shape = document.querySelector('#frame-shape');
  const margin = document.querySelector('#frame-margin');
  const show = document.querySelector('#show-frame-guide');
  const guide = document.querySelector('#frame-guide');
  const ratios = { portrait: 9 / 16, landscape: 16 / 9, classic: 4 / 3 };
  const refresh = () => {
    const ratio = ratios[shape.value];
    frame.classList.toggle('fixed-frame', ratio !== undefined);
    if (ratio !== undefined) frame.style.setProperty('--frame-ratio', String(ratio));
    margin.disabled = !show.checked;
    guide.hidden = !show.checked;
    if (margin.value !== '' && margin.validity.valid) guide.style.inset = margin.value + '%';
  };
  shape.onchange = refresh;
  margin.oninput = refresh;
  show.onchange = refresh;
  refresh();
}
