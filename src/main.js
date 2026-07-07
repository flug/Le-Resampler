const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// --- State ---
let samples = [];
let playingPath = null;
let scanning = false;
let selectedIds = new Set();
let autoPlay = false;

// --- DOM refs ---
const sampleList = document.getElementById('sample-list');
const emptyState = document.getElementById('empty-state');
const sampleTable = document.getElementById('sample-table');
const filterCategory = document.getElementById('filter-category');
const filterTag = document.getElementById('filter-tag');
const searchInput = document.getElementById('search');
const sampleCount = document.getElementById('sample-count');
const nowPlaying = document.getElementById('now-playing');
const btnStop = document.getElementById('btn-stop');
const volumeSlider = document.getElementById('volume-slider');
const btnAddFolder = document.getElementById('btn-add-folder');
const btnRefresh = document.getElementById('btn-refresh');
const exportFab = document.getElementById('export-fab');
const exportFabLabel = document.getElementById('export-fab-label');
const selectAll = document.getElementById('select-all');
const autoplayCb = document.getElementById('autoplay-cb');

// --- Error toast ---
function showError(msg) {
  let toast = document.getElementById('error-toast');
  if (!toast) {
    toast = document.createElement('div');
    toast.id = 'error-toast';
    document.body.appendChild(toast);
  }
  toast.textContent = msg;
  toast.hidden = false;
  setTimeout(() => { toast.hidden = true; }, 5000);
}

// --- Settings panel ---
const settingsPanel = document.getElementById('settings-panel');
const spTemplate = document.getElementById('sp-template');
const spClear = document.getElementById('sp-clear');
const spPreviewText = document.getElementById('sp-preview-text');
const spSave = document.getElementById('sp-save');
const spBack = document.getElementById('sp-back');
const spStatus = document.getElementById('sp-status');
const spWriteKey = document.getElementById('sp-write-key');

const SETTING_KEY = 'export_template';
const DEFAULT_TEMPLATE = '%category%/%filename%';
const SP_EXAMPLE = {
  '%filename%': 'thekick_001.wav',
  '%category%': 'kick',
  '%key%': 'Am',
  '%bpm%': '120',
  '%tags%': 'groovy',
  '%type%': 'one_shot',
};

function buildPreview(tpl) {
  let s = tpl;
  for (const [v, ex] of Object.entries(SP_EXAMPLE)) s = s.replaceAll(v, ex);
  return s || '…';
}

function spSetStatus(msg, isError = false) {
  spStatus.textContent = msg;
  spStatus.style.color = isError ? '#ffd0d0' : '#a0e8bc';
}

async function showSettings() {
  sampleTable.style.display = 'none';
  emptyState.hidden = true;
  settingsPanel.hidden = false;
  try {
    const saved = await invoke('get_setting', { key: SETTING_KEY });
    spTemplate.value = saved ?? DEFAULT_TEMPLATE;
  } catch (_) {
    spTemplate.value = DEFAULT_TEMPLATE;
  }
  spTemplate.classList.remove('sp-invalid');
  spClear.hidden = spTemplate.value === '';
  spPreviewText.textContent = buildPreview(spTemplate.value);
  spStatus.textContent = '';

  try {
    const kd = await invoke('get_setting', { key: 'key_detection' });
    const val = kd ?? 'off';
    const radio = document.querySelector(`input[name="key-detection"][value="${val}"]`);
    if (radio) radio.checked = true;
    else document.getElementById('sp-kd-off').checked = true;
  } catch (_) { document.getElementById('sp-kd-off').checked = true; }

  try {
    const writeKey = await invoke('get_setting', { key: 'write_key_to_metadata' });
    spWriteKey.checked = writeKey === 'true';
  } catch (_) { spWriteKey.checked = false; }
}

function hideSettings() {
  settingsPanel.hidden = true;
  renderSamples();
}

function insertVariable(variable) {
  const start = spTemplate.selectionStart;
  const end = spTemplate.selectionEnd;
  spTemplate.value = spTemplate.value.slice(0, start) + variable + spTemplate.value.slice(end);
  spTemplate.selectionStart = spTemplate.selectionEnd = start + variable.length;
  spTemplate.focus();
  spTemplate.dispatchEvent(new Event('input'));
}

spTemplate.addEventListener('input', () => {
  spPreviewText.textContent = buildPreview(spTemplate.value);
  spClear.hidden = spTemplate.value === '';
  spTemplate.classList.remove('sp-invalid');
  spStatus.textContent = '';
});

spClear.addEventListener('click', () => {
  spTemplate.value = '';
  spTemplate.dispatchEvent(new Event('input'));
  spTemplate.focus();
});

document.querySelectorAll('.sp-vars tbody td:first-child').forEach(td => {
  td.addEventListener('click', () => insertVariable(td.textContent.trim()));
});

spSave.addEventListener('click', async () => {
  const value = spTemplate.value.trim();
  if (!value) {
    spTemplate.classList.add('sp-invalid');
    spSetStatus('Template cannot be empty.', true);
    return;
  }
  spSave.disabled = true;
  try {
    await invoke('set_setting', { key: SETTING_KEY, value });
    const kd = document.querySelector('input[name="key-detection"]:checked')?.value ?? 'off';
    await invoke('set_setting', { key: 'key_detection', value: kd });
    await invoke('set_setting', { key: 'write_key_to_metadata', value: String(spWriteKey.checked) });
    spSetStatus('Saved.');
    setTimeout(() => { spStatus.textContent = ''; }, 2000);
  } catch (e) {
    spSetStatus('Error: ' + e, true);
  } finally {
    spSave.disabled = false;
  }
});

spBack.addEventListener('click', hideSettings);

await listen('show-settings', () => showSettings());

// --- Scan progress events ---
await listen('scan-progress', (event) => {
  const { current, total } = event.payload;
  sampleCount.textContent = `Scanning… ${current}/${total}`;
});

await listen('scan-complete', (event) => {
  const { added } = event.payload;
  scanning = false;
  btnAddFolder.disabled = false;
  btnRefresh.disabled = false;
  loadSamples();
  loadTags();
  const prev = sampleCount.textContent;
  sampleCount.textContent = `+${added} added`;
  setTimeout(() => { sampleCount.textContent = prev; loadSamples(); }, 2000);
});

// --- Add folder button ---
btnAddFolder.addEventListener('click', async () => {
  try {
    const folder = await invoke('pick_folder');
    if (!folder) return;
    scanning = true;
    btnAddFolder.disabled = true;
    sampleCount.textContent = 'Scanning…';
    await invoke('scan_folder', { path: folder });
  } catch (e) {
    scanning = false;
    btnAddFolder.disabled = false;
    showError('Scan error: ' + e);
  }
});

// --- Load and render samples ---
async function loadSamples() {
  try {
    const category = filterCategory.value || null;
    const tag = filterTag.value || null;
    const raw = searchInput.value.trim();
    const search = raw ? `%${raw}%` : null;
    samples = await invoke('list_samples', {
      filterCategory: category,
      filterTag: tag,
      search,
    });
    renderSamples();
  } catch (e) {
    showError('Failed to load samples: ' + e);
  }
}

function renderSamples() {
  if (!settingsPanel.hidden) return;
  sampleList.innerHTML = '';
  const visible = samples.length > 0;
  emptyState.hidden = visible;
  sampleTable.style.display = visible ? '' : 'none';
  sampleCount.textContent = visible ? `${samples.length} samples` : '';

  for (const s of samples) {
    const tr = document.createElement('tr');
    tr.className = 'sample-row'
      + (s.path === playingPath ? ' playing' : '')
      + (selectedIds.has(s.id) ? ' selected' : '');
    tr.dataset.path = s.path;

    // Checkbox de sélection
    const selectTd = document.createElement('td');
    selectTd.className = 'col-select';
    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.checked = selectedIds.has(s.id);
    cb.addEventListener('change', () => toggleSelection(s.id, cb.checked));
    cb.addEventListener('click', e => e.stopPropagation());
    selectTd.appendChild(cb);

    // Bouton lecture
    const playTd = document.createElement('td');
    playTd.className = 'col-play';
    const btnPlay = document.createElement('button');
    btnPlay.className = 'btn-play-row' + (s.path === playingPath ? ' playing' : '');
    btnPlay.title = s.path === playingPath ? 'Stop' : 'Preview';
    btnPlay.textContent = s.path === playingPath ? '■' : '▶';
    btnPlay.addEventListener('click', async (e) => {
      e.stopPropagation();
      if (s.path === playingPath) {
        await invoke('stop_preview');
        playingPath = null;
        nowPlaying.textContent = '—';
        btnStop.disabled = true;
      } else {
        await previewSample(s.path, s.filename);
      }
      renderSamples();
    });
    playTd.appendChild(btnPlay);
    const categoryOptions = ['', 'kick', 'snare', 'hat', 'loop', 'vocal', 'fx', 'bass', 'perc']
      .map(c => `<option value="${c}" ${s.category === c ? 'selected' : ''}>${c || '—'}</option>`)
      .join('');

    const tagsHtml = s.tags.map(tag =>
      `<span class="tag">${escHtml(tag)}<button class="rm-tag" data-id="${s.id}" data-tag="${escHtml(tag)}" title="Remove tag">×</button></span>`
    ).join('');

    const contentDiv = document.createElement('template');
    contentDiv.innerHTML = `
      <td class="filename" title="${escHtml(s.path)}">${escHtml(s.filename)}</td>
      <td><select class="edit-category" data-id="${s.id}">${categoryOptions}</select></td>
      <td><input class="edit-bpm" type="number" step="0.1" min="40" max="300"
          data-id="${s.id}" value="${s.bpm ?? ''}" placeholder="—"></td>
      <td><input class="edit-key" type="text" maxlength="8"
          data-id="${s.id}" value="${escHtml(s.musical_key ?? '')}" placeholder="—"></td>
      <td>${formatDuration(s.duration_ms)}</td>
      <td>${s.sample_type ?? '—'}</td>
      <td class="tags-cell">${tagsHtml}<input class="add-tag" type="text" placeholder="+ tag" data-id="${s.id}"></td>
    `;
    tr.appendChild(selectTd);
    tr.appendChild(playTd);
    tr.appendChild(contentDiv.content);

    tr.addEventListener('click', (e) => {
      if (['INPUT', 'SELECT', 'BUTTON'].includes(e.target.tagName)) return;
      if (autoPlay) previewSample(s.path, s.filename);
    });

    sampleList.appendChild(tr);
  }

  updateMasterCheckbox();
  updateExportButton();

  // Inline edit: category
  sampleList.querySelectorAll('.edit-category').forEach(el => {
    el.addEventListener('change', async () => {
      const row = samples.find(s => s.id === parseInt(el.dataset.id));
      try {
        await invoke('update_sample_metadata', {
          id: parseInt(el.dataset.id),
          category: el.value || null,
          bpm: row?.bpm ?? null,
          musicalKey: row?.musical_key ?? null,
        });
      } catch (e) { showError('Save failed: ' + e); }
    });
  });

  // Inline edit: BPM
  sampleList.querySelectorAll('.edit-bpm').forEach(el => {
    el.addEventListener('change', async () => {
      const row = samples.find(s => s.id === parseInt(el.dataset.id));
      const bpm = el.value ? parseFloat(el.value) : null;
      try {
        await invoke('update_sample_metadata', {
          id: parseInt(el.dataset.id),
          category: row?.category ?? null,
          bpm,
          musicalKey: row?.musical_key ?? null,
        });
      } catch (e) { showError('Save failed: ' + e); }
    });
  });

  // Inline edit: Key
  sampleList.querySelectorAll('.edit-key').forEach(el => {
    el.addEventListener('change', async () => {
      const row = samples.find(s => s.id === parseInt(el.dataset.id));
      try {
        await invoke('update_sample_metadata', {
          id: parseInt(el.dataset.id),
          category: row?.category ?? null,
          bpm: row?.bpm ?? null,
          musicalKey: el.value || null,
        });
      } catch (e) { showError('Save failed: ' + e); }
    });
  });

  // Remove tag
  sampleList.querySelectorAll('.rm-tag').forEach(el => {
    el.addEventListener('click', async (e) => {
      e.stopPropagation();
      try {
        await invoke('remove_tag_from_sample', {
          sampleId: parseInt(el.dataset.id),
          tagName: el.dataset.tag,
        });
        loadSamples();
      } catch (e) { showError('Remove tag failed: ' + e); }
    });
  });

  // Add tag on Enter
  sampleList.querySelectorAll('.add-tag').forEach(el => {
    el.addEventListener('keydown', async (e) => {
      if (e.key !== 'Enter') return;
      const tag = el.value.trim();
      if (!tag) return;
      try {
        await invoke('add_tag_to_sample', {
          sampleId: parseInt(el.dataset.id),
          tagName: tag,
        });
        el.value = '';
        loadSamples();
        loadTags();
      } catch (err) { showError('Add tag failed: ' + err); }
    });
    el.addEventListener('click', e => e.stopPropagation());
  });
}

async function loadTags() {
  try {
    const tags = await invoke('get_all_tags');
    const currentVal = filterTag.value;
    filterTag.innerHTML = '<option value="">All Tags</option>';
    for (const tag of tags) {
      const opt = document.createElement('option');
      opt.value = tag;
      opt.textContent = tag;
      if (tag === currentVal) opt.selected = true;
      filterTag.appendChild(opt);
    }
  } catch (e) {
    showError('Failed to load tags: ' + e);
  }
}

async function previewSample(path, filename) {
  try {
    playingPath = path;
    nowPlaying.textContent = '▶ ' + filename;
    btnStop.disabled = false;
    renderSamples();
    await invoke('preview_sample', { path });
  } catch (e) {
    showError('Playback error: ' + e);
  }
}

btnStop.addEventListener('click', async () => {
  try {
    await invoke('stop_preview');
    playingPath = null;
    nowPlaying.textContent = '—';
    btnStop.disabled = true;
    renderSamples();
  } catch (e) {
    showError('Stop error: ' + e);
  }
});

// Space key → stop preview
document.addEventListener('keydown', async (e) => {
  if (e.code === 'Space' && e.target === document.body) {
    e.preventDefault();
    if (playingPath) btnStop.click();
  }
});

// Volume slider
volumeSlider.addEventListener('input', async () => {
  try {
    await invoke('set_volume', { volume: parseFloat(volumeSlider.value) });
  } catch (e) {
    showError('Volume error: ' + e);
  }
});

// Appliquer le volume initial au démarrage
invoke('set_volume', { volume: parseFloat(volumeSlider.value) }).catch(() => {});

// --- Refresh button ---
btnRefresh.addEventListener('click', async () => {
  if (scanning) return;
  try {
    const folders = await invoke('get_watched_folders');
    if (folders.length === 0) {
      showToast('No folders saved yet.');
      return;
    }
    scanning = true;
    btnAddFolder.disabled = true;
    btnRefresh.disabled = true;
    sampleCount.textContent = 'Scanning…';
    for (const folder of folders) {
      await invoke('scan_folder', { path: folder });
    }
  } catch (e) {
    showError('Refresh error: ' + e);
    scanning = false;
    btnAddFolder.disabled = false;
    btnRefresh.disabled = false;
  }
});

// --- Auto-play toggle ---
autoplayCb.addEventListener('change', () => { autoPlay = autoplayCb.checked; });

// --- Filters ---
filterCategory.addEventListener('change', loadSamples);
filterTag.addEventListener('change', loadSamples);

let searchTimeout;
searchInput.addEventListener('input', () => {
  clearTimeout(searchTimeout);
  searchTimeout = setTimeout(loadSamples, 300);
});

// --- Helpers ---
function formatDuration(ms) {
  if (!ms) return '—';
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

function escHtml(str) {
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

// --- Selection ---
function toggleSelection(id, checked) {
  if (checked) selectedIds.add(id);
  else selectedIds.delete(id);
  updateExportButton();
  updateMasterCheckbox();
}

function updateMasterCheckbox() {
  const visibleIds = samples.map(s => s.id);
  const checkedCount = visibleIds.filter(id => selectedIds.has(id)).length;
  selectAll.indeterminate = checkedCount > 0 && checkedCount < visibleIds.length;
  selectAll.checked = visibleIds.length > 0 && checkedCount === visibleIds.length;
}

function updateExportButton() {
  const n = selectedIds.size;
  exportFab.hidden = n === 0;
  exportFabLabel.textContent = `Export (${n})`;
}

selectAll.addEventListener('change', () => {
  const visibleIds = samples.map(s => s.id);
  if (selectAll.checked) visibleIds.forEach(id => selectedIds.add(id));
  else visibleIds.forEach(id => selectedIds.delete(id));
  renderSamples();
});

// --- Export ---
async function exportSamples() {
  const dest = await invoke('pick_folder');
  if (!dest) return;

  const template = await invoke('get_setting', { key: 'export_template' })
    .catch(() => null) ?? '%category%/%filename%';

  const entries = samples
    .filter(s => selectedIds.has(s.id))
    .map(s => ({
      path: s.path,
      filename: s.filename,
      category: s.category,
      tags: s.tags,
      bpm: s.bpm ?? null,
      musical_key: s.musical_key ?? null,
      sample_type: s.sample_type ?? null,
    }));

  exportFab.disabled = true;
  try {
    const r = await invoke('copy_samples_to', { dest, samples: entries, template });
    const msg = `✓ ${r.copied} sample(s) copied${r.skipped ? `, ${r.skipped} skipped` : ''}`;
    showToast(msg);
    if (r.errors.length > 0) {
      showError(`Export errors:\n${r.errors.slice(0, 3).join('\n')}`);
    }
  } catch (e) {
    showError('Export error: ' + e);
  } finally {
    exportFab.disabled = false;
    updateExportButton();
  }
}

exportFab.addEventListener('click', exportSamples);

// --- Toast success ---
function showToast(msg) {
  let toast = document.getElementById('success-toast');
  if (!toast) {
    toast = document.createElement('div');
    toast.id = 'success-toast';
    document.body.appendChild(toast);
  }
  toast.textContent = msg;
  toast.hidden = false;
  clearTimeout(toast._t);
  toast._t = setTimeout(() => { toast.hidden = true; }, 4000);
}

// --- Update check ---
const updateNotice = document.getElementById('update-notice');

function parseSemver(v) {
  return v.replace(/^v/, '').split('.').map(Number);
}

function isNewer(latest, current) {
  const [lMaj, lMin, lPatch] = parseSemver(latest);
  const [cMaj, cMin, cPatch] = parseSemver(current);
  if (lMaj !== cMaj) return lMaj > cMaj;
  if (lMin !== cMin) return lMin > cMin;
  return lPatch > cPatch;
}

async function checkForUpdate() {
  try {
    const currentVersion = await invoke('get_app_version');
    const res = await fetch(
      'https://api.github.com/repos/flugv1/Le-Resampler/releases/latest',
      { headers: { Accept: 'application/vnd.github.v3+json' } }
    );
    if (!res.ok) return;
    const { tag_name, html_url } = await res.json();
    if (isNewer(tag_name, currentVersion)) {
      updateNotice.textContent = `↑ v${tag_name.replace(/^v/, '')} available`;
      updateNotice.hidden = false;
      updateNotice.onclick = () => invoke('open_url', { url: html_url });
    }
  } catch (_) {}
}

// --- Init ---
loadSamples();
loadTags();
checkForUpdate();
