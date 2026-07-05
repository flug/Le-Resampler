const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// --- State ---
let samples = [];
let playingPath = null;
let scanning = false;
let selectedIds = new Set();

// --- DOM refs ---
const sampleList = document.getElementById('sample-list');
const emptyState = document.getElementById('empty-state');
const sampleTable = document.getElementById('sample-table');
const filterCategory = document.getElementById('filter-category');
const filterTag = document.getElementById('filter-tag');
const searchInput = document.getElementById('search');
const progressWrap = document.getElementById('progress-wrap');
const progressEl = document.getElementById('progress');
const progressLabel = document.getElementById('progress-label');
const sampleCount = document.getElementById('sample-count');
const nowPlaying = document.getElementById('now-playing');
const btnStop = document.getElementById('btn-stop');
const btnAddFolder = document.getElementById('btn-add-folder');
const btnExport = document.getElementById('export-btn');
const selectAll = document.getElementById('select-all');

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

// --- Scan progress events ---
await listen('scan-progress', (event) => {
  const { current, total, filename } = event.payload;
  progressWrap.hidden = false;
  progressEl.max = total || 1;
  progressEl.value = current;
  progressLabel.textContent = `${current}/${total}${filename ? ': ' + filename : ''}`;
});

await listen('scan-complete', (event) => {
  const { added, skipped } = event.payload;
  progressWrap.hidden = true;
  scanning = false;
  btnAddFolder.disabled = false;
  loadSamples();
  loadTags();
  // Brief status in count area
  const prev = sampleCount.textContent;
  sampleCount.textContent = `+${added} added`;
  setTimeout(() => { sampleCount.textContent = prev; loadSamples(); }, 1500);
});

// --- Add folder button ---
btnAddFolder.addEventListener('click', async () => {
  try {
    const folder = await invoke('pick_folder');
    if (!folder) return;
    scanning = true;
    btnAddFolder.disabled = true;
    progressWrap.hidden = false;
    progressEl.value = 0;
    progressLabel.textContent = 'Starting scan…';
    await invoke('scan_folder', { path: folder });
  } catch (e) {
    scanning = false;
    btnAddFolder.disabled = false;
    progressWrap.hidden = true;
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
    tr.appendChild(contentDiv.content);

    // Row click → preview (ignore clicks on inputs/selects/buttons)
    tr.addEventListener('click', (e) => {
      if (['INPUT', 'SELECT', 'BUTTON'].includes(e.target.tagName)) return;
      previewSample(s.path, s.filename);
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
  btnExport.disabled = n === 0;
  btnExport.textContent = n > 0 ? `Exporter (${n})` : 'Exporter';
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

  const entries = samples
    .filter(s => selectedIds.has(s.id))
    .map(s => ({ path: s.path, category: s.category, tags: s.tags }));

  btnExport.disabled = true;
  try {
    const r = await invoke('copy_samples_to', { dest, samples: entries });
    const msg = `✓ ${r.copied} sample(s) copié(s)${r.skipped ? `, ${r.skipped} ignoré(s)` : ''}`;
    showToast(msg);
    if (r.errors.length > 0) {
      showError(`Erreurs lors de l'export :\n${r.errors.slice(0, 3).join('\n')}`);
    }
  } catch (e) {
    showError('Erreur export : ' + e);
  } finally {
    updateExportButton();
  }
}

btnExport.addEventListener('click', exportSamples);

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

// --- Init ---
loadSamples();
loadTags();
