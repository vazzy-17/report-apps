let currentServers = [];
let selectedServer = null;
let currentTab = 'table-browser';
let currentTableSubTab = 'data';
let activeTableName = '';
let currentDataList = []; // Caches the current data for CSV export
let tableSearchTerm = '';
let allTables = [];

document.addEventListener('DOMContentLoaded', () => {
  init();
});

function init() {
  lucide.createIcons();
  
  // Check if JWT token exists in localStorage
  const token = localStorage.getItem('db_token');
  const username = localStorage.getItem('db_username');
  const dbname = localStorage.getItem('db_dbname');
  const serverName = localStorage.getItem('db_server_name');

  if (token && username && dbname && serverName) {
    showDashboard(token, username, dbname, serverName);
  } else {
    showAuth();
  }

  // Bind Ctrl+Enter for SQL Query Editor
  const sqlInput = document.getElementById('sql-query-input');
  if (sqlInput) {
    sqlInput.addEventListener('keydown', (e) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
        e.preventDefault();
        runCustomQuery();
      }
    });
  }
}

function showAuth() {
  document.getElementById('auth-view').classList.remove('hidden');
  document.getElementById('dashboard-view').classList.add('hidden');
  loadServers();
}

function showDashboard(token, username, dbname, serverName) {
  document.getElementById('auth-view').classList.add('hidden');
  document.getElementById('dashboard-view').classList.remove('hidden');

  // Update header labels
  document.getElementById('dash-server-name').innerText = serverName;
  document.getElementById('dash-dbname').innerText = dbname;
  document.getElementById('dash-username').innerText = username;

  // Clear workspace variables
  activeTableName = '';
  currentDataList = [];
  document.getElementById('active-table-title').classList.add('hidden');
  document.getElementById('workspace-placeholder').classList.remove('hidden');
  document.getElementById('tab-table-browser').classList.add('hidden');
  
  // Load tables
  loadTables(token);
}

// Fetch list of servers from backend
async function loadServers() {
  try {
    const res = await fetch('/api/servers');
    if (!res.ok) throw new Error('Gagal memuat daftar server');
    
    currentServers = await res.json();
    const select = document.getElementById('server-select');
    select.innerHTML = '<option value="" disabled selected>-- Pilih Server / Database --</option>';
    
    currentServers.forEach(server => {
      const opt = document.createElement('option');
      opt.value = server.id;
      opt.textContent = server.name;
      select.appendChild(opt);
    });
    
    updateServerInfo();
  } catch (err) {
    console.error(err);
    document.getElementById('login-error').classList.remove('hidden');
    document.getElementById('login-error-msg').innerText = 'Tidak dapat terhubung dengan backend Rust atau databases.json tidak ditemukan.';
  }
}

// Show info on selected server
function updateServerInfo() {
  const select = document.getElementById('server-select');
  const serverId = select.value;
  selectedServer = currentServers.find(s => s.id === serverId);

  const preview = document.getElementById('server-info-preview');
  if (selectedServer) {
    preview.classList.remove('hidden');
    document.getElementById('info-host').innerText = `${selectedServer.host}:${selectedServer.port}`;
    document.getElementById('info-db').innerText = selectedServer.dbname;
  } else {
    preview.classList.add('hidden');
  }
}

async function handleLogin(e) {
  e.preventDefault();
  
  const select = document.getElementById('server-select');
  const serverId = select.value;

  if (!serverId) {
    showLoginError('Silakan pilih server terlebih dahulu.');
    return;
  }

  // Show loading spinner
  const btnText = document.getElementById('btn-login-text');
  const spinner = document.getElementById('btn-login-spinner');
  btnText.innerText = 'Menghubungkan...';
  spinner.classList.remove('hidden');
  document.getElementById('login-error').classList.add('hidden');

  try {
    const response = await fetch('/api/auth/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ server_id: serverId })
    });

    const data = await response.json();

    if (!response.ok) {
      throw new Error(data.error || 'Autentikasi gagal.');
    }

    // Save tokens and user details
    localStorage.setItem('db_token', data.token);
    localStorage.setItem('db_username', data.username);
    localStorage.setItem('db_dbname', data.dbname);
    localStorage.setItem('db_server_name', data.server_name);

    showDashboard(data.token, data.username, data.dbname, data.server_name);
  } catch (err) {
    showLoginError(err.message);
  } finally {
    btnText.innerText = 'Sambungkan & Masuk';
    spinner.classList.add('hidden');
  }
}

function showLoginError(msg) {
  const errDiv = document.getElementById('login-error');
  errDiv.classList.remove('hidden');
  document.getElementById('login-error-msg').innerText = msg;
}

function handleLogout() {
  localStorage.removeItem('db_token');
  localStorage.removeItem('db_username');
  localStorage.removeItem('db_dbname');
  localStorage.removeItem('db_server_name');
  showAuth();
}

// Load database tables list
async function loadTables(token) {
  const container = document.getElementById('tables-list');
  container.innerHTML = '<div class="text-center py-8 text-xs text-slate-500">Memuat tabel database...</div>';

  try {
    const response = await fetch('/api/tables', {
      headers: { 'Authorization': `Bearer ${token}` }
    });

    const data = await response.json();
    if (!response.ok) throw new Error(data.error || 'Gagal memuat daftar tabel.');

    allTables = data;
    renderTablesList(allTables);
  } catch (err) {
    container.innerHTML = `<div class="text-center py-8 text-xs text-red-400 border border-red-950/40 rounded-lg p-3 bg-red-950/20">
      <i data-lucide="alert-triangle" class="w-5 h-5 mx-auto text-red-500 mb-1"></i>
      Sesi berakhir atau gagal terhubung: <br>${err.message}
    </div>`;
    lucide.createIcons();
  }
}

// Render dynamic list of tables in sidebar
function renderTablesList(tables) {
  const container = document.getElementById('tables-list');
  container.innerHTML = '';

  if (tables.length === 0) {
    container.innerHTML = '<div class="text-center py-8 text-xs text-slate-500">Tidak ada tabel public ditemukan.</div>';
    return;
  }

  tables.forEach(tbl => {
    const button = document.createElement('button');
    button.onclick = () => selectTable(tbl.table_name);
    button.className = `w-full text-left p-2.5 rounded-lg flex flex-col gap-1 transition ${
      activeTableName === tbl.table_name ? 'bg-indigo-600/10 border border-indigo-500/20 text-indigo-400' : 'hover:bg-slate-900 border border-transparent text-slate-400 hover:text-slate-200'
    }`;

    // Header info (Table Name & Type badge)
    const header = document.createElement('div');
    header.className = 'flex justify-between items-center w-full';
    
    const nameSpan = document.createElement('span');
    nameSpan.className = 'font-mono text-xs truncate max-w-[170px] text-slate-200';
    nameSpan.title = tbl.table_name;
    nameSpan.innerText = tbl.table_name;

    const badge = document.createElement('span');
    const isView = tbl.table_type === 'VIEW';
    badge.className = `text-[9px] px-1.5 py-0.5 rounded font-bold ${
      isView ? 'bg-sky-950 text-sky-400 border border-sky-900/50' : 'bg-indigo-950 text-indigo-400 border border-indigo-900/50'
    }`;
    badge.innerText = isView ? 'VIEW' : 'TBL';

    header.appendChild(nameSpan);
    header.appendChild(badge);

    // Row / size metrics
    const metrics = document.createElement('div');
    metrics.className = 'flex justify-between items-center text-[10px] text-slate-500';
    
    const rowSpan = document.createElement('span');
    rowSpan.innerText = `${tbl.row_count.toLocaleString()} baris`;
    
    const sizeSpan = document.createElement('span');
    sizeSpan.innerText = tbl.total_size;

    metrics.appendChild(rowSpan);
    metrics.appendChild(sizeSpan);

    button.appendChild(header);
    button.appendChild(metrics);

    container.appendChild(button);
  });
}

function filterTables() {
  const val = document.getElementById('table-search').value.toLowerCase();
  const filtered = allTables.filter(t => t.table_name.toLowerCase().includes(val));
  renderTablesList(filtered);
}

// Action when user selects a table
async function selectTable(tableName) {
  activeTableName = tableName;
  
  // Highlight active table in sidebar
  filterTables();

  // Show active label in headers
  document.getElementById('active-table-title').classList.remove('hidden');
  document.getElementById('active-table-name').innerText = tableName;

  // Toggle UI view
  document.getElementById('workspace-placeholder').classList.add('hidden');
  document.getElementById('tab-table-browser').classList.remove('hidden');

  // Load the selected table's schema and preview data
  switchTab('table-browser');
  
  // Load data for the sub-tab (Data Preview is selected by default)
  loadTableData();
  loadTableSchema();
}

// Fetch table row data preview
async function loadTableData() {
  const tableHead = document.getElementById('preview-table-head');
  const tableBody = document.getElementById('preview-table-body');
  
  tableHead.innerHTML = '';
  tableBody.innerHTML = '<tr><td colspan="100" class="text-center py-12 text-slate-500 text-xs">Memuat baris data...</td></tr>';

  const token = localStorage.getItem('db_token');

  try {
    const res = await fetch(`/api/table/${activeTableName}/data`, {
      headers: { 'Authorization': `Bearer ${token}` }
    });

    const data = await res.json();
    if (!res.ok) throw new Error(data.error || 'Gagal memuat data tabel.');

    currentDataList = data;
    renderTableData(data, tableHead, tableBody);
  } catch (err) {
    tableBody.innerHTML = `<tr><td colspan="100" class="text-center py-12 text-red-400 font-mono text-xs">${err.message}</td></tr>`;
  }
}

// Fetch columns schema
async function loadTableSchema() {
  const schemaBody = document.getElementById('schema-table-body');
  schemaBody.innerHTML = '<tr><td colspan="4" class="text-center py-12 text-slate-500 text-xs">Memuat skema kolom...</td></tr>';

  const token = localStorage.getItem('db_token');

  try {
    const res = await fetch(`/api/table/${activeTableName}/schema`, {
      headers: { 'Authorization': `Bearer ${token}` }
    });

    const data = await res.json();
    if (!res.ok) throw new Error(data.error || 'Gagal memuat skema.');

    schemaBody.innerHTML = '';
    data.forEach(col => {
      const tr = document.createElement('tr');
      tr.className = 'hover:bg-slate-900/30';

      tr.innerHTML = `
        <td class="whitespace-nowrap py-3.5 pl-6 pr-3 text-sm font-mono text-slate-200">${col.column_name}</td>
        <td class="whitespace-nowrap px-3 py-3.5 text-sm font-mono text-indigo-400">${col.data_type}</td>
        <td class="whitespace-nowrap px-3 py-3.5 text-sm text-slate-400">${col.is_nullable}</td>
        <td class="whitespace-nowrap px-3 py-3.5 text-sm font-mono text-slate-500">${col.column_default || '-'}</td>
      `;
      schemaBody.appendChild(tr);
    });
  } catch (err) {
    schemaBody.innerHTML = `<tr><td colspan="4" class="text-center py-12 text-red-400 font-mono text-xs">${err.message}</td></tr>`;
  }
}

// Render generic JSON rows into an HTML table representation
function renderTableData(data, headElem, bodyElem) {
  headElem.innerHTML = '';
  bodyElem.innerHTML = '';

  if (data.length === 0) {
    bodyElem.innerHTML = '<tr><td colspan="100" class="text-center py-12 text-slate-500 text-xs">Tabel ini tidak memiliki baris data.</td></tr>';
    return;
  }

  // Construct headers dynamically from JSON object keys
  const keys = Object.keys(data[0]);
  
  const trHead = document.createElement('tr');
  keys.forEach(key => {
    const th = document.createElement('th');
    th.scope = 'col';
    th.className = 'px-4 py-3 text-left text-xs font-semibold uppercase tracking-wider text-slate-400 font-mono border-b border-slate-900';
    th.innerText = key;
    trHead.appendChild(th);
  });
  headElem.appendChild(trHead);

  // Construct rows
  data.forEach(row => {
    const trRow = document.createElement('tr');
    trRow.className = 'hover:bg-slate-900/40 divide-x divide-slate-900/30';
    
    keys.forEach(key => {
      const td = document.createElement('td');
      td.className = 'px-4 py-2.5 text-xs font-mono truncate max-w-[220px] text-slate-300';
      const val = row[key];
      
      if (val === null || val === undefined) {
        td.className += ' text-slate-600 italic';
        td.innerText = 'null';
      } else if (typeof val === 'object') {
        td.innerText = JSON.stringify(val);
        td.title = JSON.stringify(val);
      } else {
        td.innerText = val;
        td.title = val;
      }
      trRow.appendChild(td);
    });
    bodyElem.appendChild(trRow);
  });
}

// Switch between Main Tabs
function switchTab(tabName) {
  currentTab = tabName;

  const tabs = ['table-browser', 'query-editor'];
  tabs.forEach(t => {
    const btn = document.getElementById(`tab-btn-${t}`);
    const area = document.getElementById(`tab-${t}`);
    
    if (t === tabName) {
      btn.className = 'border-b-2 border-indigo-500 text-indigo-400 px-1 py-4 text-sm font-semibold tracking-wide flex items-center gap-2';
      area.classList.remove('hidden');
    } else {
      btn.className = 'border-b-2 border-transparent text-slate-400 hover:text-slate-300 hover:border-slate-800 px-1 py-4 text-sm font-semibold tracking-wide flex items-center gap-2';
      area.classList.add('hidden');
    }
  });

  // Adjust placeholder
  if (tabName === 'table-browser' && !activeTableName) {
    document.getElementById('workspace-placeholder').classList.remove('hidden');
    document.getElementById('tab-table-browser').classList.add('hidden');
  } else {
    document.getElementById('workspace-placeholder').classList.add('hidden');
  }
}

// Switch Table Sub-tabs
function switchTableSubTab(subTab) {
  currentTableSubTab = subTab;
  
  const btnData = document.getElementById('btn-show-data');
  const btnSchema = document.getElementById('btn-show-schema');
  
  const dataView = document.getElementById('table-data-view');
  const schemaView = document.getElementById('table-schema-view');

  if (subTab === 'data') {
    btnData.className = 'text-xs font-semibold bg-indigo-600 text-white px-3 py-1.5 rounded-md shadow';
    btnSchema.className = 'text-xs font-semibold bg-slate-900 text-slate-400 hover:text-slate-300 px-3 py-1.5 rounded-md border border-slate-800';
    dataView.classList.remove('hidden');
    schemaView.classList.add('hidden');
  } else {
    btnSchema.className = 'text-xs font-semibold bg-indigo-600 text-white px-3 py-1.5 rounded-md shadow';
    btnData.className = 'text-xs font-semibold bg-slate-900 text-slate-400 hover:text-slate-300 px-3 py-1.5 rounded-md border border-slate-800';
    schemaView.classList.remove('hidden');
    dataView.classList.add('hidden');
  }
}

// Run dynamic SQL query
async function runCustomQuery() {
  const query = document.getElementById('sql-query-input').value.trim();
  if (!query) return;

  const btn = document.getElementById('btn-run-query');
  const spinner = document.getElementById('btn-run-spinner');
  const errDiv = document.getElementById('query-error');
  const emptyState = document.getElementById('query-empty-state');
  const table = document.getElementById('query-table');
  const head = document.getElementById('query-table-head');
  const body = document.getElementById('query-table-body');
  const exportBtn = document.getElementById('btn-export-query');

  // Loading UI
  btn.disabled = true;
  spinner.classList.remove('hidden');
  errDiv.classList.add('hidden');
  table.classList.add('hidden');
  emptyState.classList.add('hidden');
  exportBtn.classList.add('hidden');

  const token = localStorage.getItem('db_token');

  try {
    const response = await fetch('/api/query', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${token}`
      },
      body: JSON.stringify({ query })
    });

    const data = await response.json();

    if (!response.ok) {
      throw new Error(data.error || 'Eksekusi query gagal.');
    }

    currentDataList = data; // Cache results for export
    table.classList.remove('hidden');
    exportBtn.classList.remove('hidden');
    renderTableData(data, head, body);
  } catch (err) {
    errDiv.classList.remove('hidden');
    errDiv.innerText = err.message;
  } finally {
    btn.disabled = false;
    spinner.classList.add('hidden');
  }
}

function clearQuery() {
  document.getElementById('sql-query-input').value = '';
  document.getElementById('query-error').classList.add('hidden');
  document.getElementById('query-table').classList.add('hidden');
  document.getElementById('btn-export-query').classList.add('hidden');
  document.getElementById('query-empty-state').classList.remove('hidden');
}

// Convert JSON Cache to CSV and trigger file download
function exportData() {
  if (!currentDataList || currentDataList.length === 0) return;

  const keys = Object.keys(currentDataList[0]);
  
  // Create CSV Header
  let csvContent = keys.map(k => `"${k.replace(/"/g, '""')}"`).join(',') + '\n';

  // Create rows content
  currentDataList.forEach(row => {
    let rowContent = keys.map(key => {
      let val = row[key];
      if (val === null || val === undefined) return '';
      if (typeof val === 'object') val = JSON.stringify(val);
      return `"${String(val).replace(/"/g, '""')}"`;
    }).join(',');
    csvContent += rowContent + '\n';
  });

  // Create downloadable file blob
  const blob = new Blob([csvContent], { type: 'text/csv;charset=utf-8;' });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  
  const filename = currentTab === 'table-browser' 
    ? `${activeTableName}_export.csv`
    : `query_export_${new Date().toISOString().slice(0, 10)}.csv`;

  link.setAttribute('href', url);
  link.setAttribute('download', filename);
  link.style.visibility = 'hidden';
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
}
