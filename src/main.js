const { invoke } = window.__TAURI__.core;

// --- 元素获取 ---
let homeBtn, folderBtn, settingsBtn, layoutToggleBtn;
let selectFolderBtn;
let homeView, folderView, settingsView;
let dirTree, breadcrumb, imageGallery, scrollContainer;
let currentFolderInfo;
let xorKeyInput, aesKeyInput, saveKeysBtn, saveStatus;
let layoutIconWaterfall, layoutIconGrid;

// --- 状态管理 ---
let currentRootDir = null;
let allImagePaths = [];
let currentImageIndex = 0;
let columnElements = [];
let sentinelObserver = null;
let isLoading = false;
let currentLayout = 'waterfall'; // 'waterfall' 或 'grid'
const BATCH_SIZE = 10;

// --- 工具函数 ---
function debounce(func, delay) {
    let timeout;
    return function (...args) {
        clearTimeout(timeout);
        timeout = setTimeout(() => func.apply(this, args), delay);
    };
}

// --- 视图切换 ---
function switchView(viewName) {
    homeView.classList.toggle('hidden', viewName !== 'home');
    folderView.classList.toggle('hidden', viewName !== 'folder');
    settingsView.classList.toggle('hidden', viewName !== 'settings');

    homeBtn.appearance = viewName === 'home' ? 'accent' : 'stealth';
    folderBtn.appearance = viewName === 'folder' ? 'accent' : 'stealth';
    settingsBtn.appearance = viewName === 'settings' ? 'accent' : 'stealth';
}

// --- 布局切换 ---
function toggleLayout() {
    currentLayout = currentLayout === 'waterfall' ? 'grid' : 'waterfall';

    // 更新图标显示
    if (currentLayout === 'grid') {
        layoutIconWaterfall.style.display = 'none';
        layoutIconGrid.style.display = 'block';
    } else {
        layoutIconWaterfall.style.display = 'block';
        layoutIconGrid.style.display = 'none';
    }

    // 重新加载当前文件夹
    const folderPath = breadcrumb.querySelector('fluent-breadcrumb-item:last-child')?.dataset.path;
    if (folderPath) {
        startImageLoading(folderPath);
    }
}

// --- 文件夹选择 ---
async function selectFolder() {
    try {
        const path = await invoke('open_folder_dialog');
        currentRootDir = path;
        currentFolderInfo.textContent = `当前目录: ${path}`;

        // 加载设置
        await loadSettings();

        // 切换到文件夹视图
        switchView('folder');
        await loadAndRenderDirectoryTree();
    } catch (error) {
        console.error('选择文件夹失败:', error);
        alert('选择文件夹失败: ' + error);
    }
}

// --- 目录树 ---
async function loadAndRenderDirectoryTree() {
    try {
        const treeData = await invoke('get_folder_tree');
        if (treeData) {
            dirTree.innerHTML = '';
            const rootNode = createTreeNode(treeData);
            dirTree.appendChild(rootNode);
        }
    } catch (error) {
        console.error('加载目录树失败:', error);
    }
}

function createTreeNode(nodeData) {
    const treeItem = document.createElement('fluent-tree-item');
    treeItem.dataset.path = nodeData.path;
    treeItem.dataset.name = nodeData.name;
    treeItem.textContent = nodeData.name;

    if (nodeData.children && nodeData.children.length > 0) {
        nodeData.children.forEach(child => {
            treeItem.appendChild(createTreeNode(child));
        });
    }

    return treeItem;
}

function selectTreeNode(nodeElement) {
    if (!nodeElement) return;

    // 展开父节点
    let parent = nodeElement.parentElement;
    while (parent && parent.tagName === 'FLUENT-TREE-ITEM') {
        parent.expanded = true;
        parent = parent.parentElement;
    }

    // 选中当前节点
    dirTree.querySelectorAll('fluent-tree-item').forEach(item => item.selected = false);
    nodeElement.selected = true;

    const folderPath = nodeElement.dataset.path;
    if (folderPath) {
        updateBreadcrumb(folderPath);
        startImageLoading(folderPath);
    }
}

// --- 面包屑导航 ---
function updateBreadcrumb(path) {
    breadcrumb.innerHTML = '';

    if (!currentRootDir || !path.startsWith(currentRootDir)) {
        const item = document.createElement('fluent-breadcrumb-item');
        item.textContent = '选择文件夹';
        breadcrumb.appendChild(item);
        return;
    }

    let runningPath = currentRootDir;
    const rootName = currentRootDir.split(/[\\/]/).pop();
    const rootItem = document.createElement('fluent-breadcrumb-item');
    rootItem.textContent = rootName;
    rootItem.dataset.path = runningPath;
    breadcrumb.appendChild(rootItem);

    const relativePath = path.substring(currentRootDir.length);
    const parts = relativePath.split(/[\\/]/).filter(p => p);

    parts.forEach((part, index) => {
        runningPath += (runningPath.endsWith('\\') || runningPath.endsWith('/') ? '' : '\\') + part;
        const item = document.createElement('fluent-breadcrumb-item');
        item.textContent = part;
        item.dataset.path = runningPath;
        breadcrumb.appendChild(item);
    });
}

// --- 图片加载 ---
async function startImageLoading(folderPath) {
    try {
        // 重置状态
        allImagePaths = [];
        currentImageIndex = 0;
        imageGallery.innerHTML = '';
        columnElements = [];

        // 获取图片列表
        allImagePaths = await invoke('get_images_in_folder', { folderPath });

        if (allImagePaths.length === 0) {
            imageGallery.innerHTML = '<p style="text-align:center;padding:20px;">该文件夹中没有图片</p>';
            return;
        }

        // 设置布局
        setupLayout();

        // 加载初始图片
        loadInitialImages();
    } catch (error) {
        console.error('加载图片失败:', error);
        imageGallery.innerHTML = '<p style="text-align:center;padding:20px;color:red;">加载失败: ' + error + '</p>';
    }
}

function setupLayout() {
    imageGallery.innerHTML = '';
    columnElements = [];

    if (currentLayout === 'grid') {
        // 网格布局
        imageGallery.classList.add('grid-layout');
        // 在网格布局中，不需要创建列，直接使用 grid
    } else {
        // 瀑布流布局
        imageGallery.classList.remove('grid-layout');

        // 创建列
        const columnCount = Math.max(2, Math.floor(imageGallery.offsetWidth / 300));

        for (let i = 0; i < columnCount; i++) {
            const column = document.createElement('div');
            column.className = 'image-column';
            imageGallery.appendChild(column);
            columnElements.push(column);
        }
    }

    // 设置哨兵观察器
    setupSentinel();
}

function setupSentinel() {
    if (sentinelObserver) {
        sentinelObserver.disconnect();
    }

    const sentinel = document.createElement('div');
    sentinel.id = 'sentinel';
    sentinel.style.height = '1px';
    scrollContainer.appendChild(sentinel);

    sentinelObserver = new IntersectionObserver((entries) => {
        if (entries[0].isIntersecting && !isLoading) {
            loadMoreImages();
        }
    }, { root: scrollContainer, threshold: 0.1 });

    sentinelObserver.observe(sentinel);
}

function loadInitialImages() {
    const viewportHeight = scrollContainer.offsetHeight;

    if (currentLayout === 'grid') {
        // 网格布局：计算需要填充视口的图片数量
        const itemWidth = 200; // minmax(200px, 1fr)
        const gap = 16;
        const columns = Math.floor(imageGallery.offsetWidth / (itemWidth + gap));
        const rows = Math.ceil(viewportHeight / (itemWidth + gap));
        const estimatedImagesNeeded = columns * rows * 2; // 多加载一些

        for (let i = 0; i < Math.min(estimatedImagesNeeded, allImagePaths.length); i++) {
            addImageToGallery();
        }
    } else {
        // 瀑布流布局
        const estimatedImagesNeeded = Math.ceil(viewportHeight / 200) * columnElements.length;

        for (let i = 0; i < Math.min(estimatedImagesNeeded, allImagePaths.length); i++) {
            const shortestColumn = getShortestColumn();
            addImageToColumn(shortestColumn);
        }
    }
}

function loadMoreImages() {
    if (isLoading || currentImageIndex >= allImagePaths.length) return;

    isLoading = true;

    for (let i = 0; i < BATCH_SIZE && currentImageIndex < allImagePaths.length; i++) {
        if (currentLayout === 'grid') {
            addImageToGallery();
        } else {
            const shortestColumn = getShortestColumn();
            addImageToColumn(shortestColumn);
        }
    }

    isLoading = false;
}

function getShortestColumn() {
    return columnElements.reduce((shortest, current) =>
        current.offsetHeight < shortest.offsetHeight ? current : shortest
    );
}

function addImageToGallery() {
    // 用于网格布局：直接添加到 gallery
    if (currentImageIndex >= allImagePaths.length) return;

    const relPath = allImagePaths[currentImageIndex++];
    const fileName = relPath.split(/[\\/]/).pop();

    const card = document.createElement('fluent-card');
    card.className = 'image-card is-loading';

    const placeholder = document.createElement('div');
    placeholder.className = 'image-placeholder';
    placeholder.textContent = '加载中...';

    const img = document.createElement('img');
    img.alt = fileName;
    img.style.display = 'none';

    const caption = document.createElement('div');
    caption.className = 'caption';
    caption.textContent = fileName;

    card.appendChild(placeholder);
    card.appendChild(img);
    card.appendChild(caption);
    imageGallery.appendChild(card);

    // 异步加载图片
    fetchAndSetImage(relPath, img, card, placeholder);
}

async function addImageToColumn(column) {
    if (currentImageIndex >= allImagePaths.length) return;

    const relPath = allImagePaths[currentImageIndex++];
    const fileName = relPath.split(/[\\/]/).pop();

    const card = document.createElement('fluent-card');
    card.className = 'image-card is-loading';

    const placeholder = document.createElement('div');
    placeholder.className = 'image-placeholder';
    placeholder.textContent = '加载中...';

    const img = document.createElement('img');
    img.alt = fileName;
    img.style.display = 'none';

    const caption = document.createElement('div');
    caption.className = 'caption';
    caption.textContent = fileName;

    card.appendChild(placeholder);
    card.appendChild(img);
    card.appendChild(caption);
    column.appendChild(card);

    // 异步加载图片
    fetchAndSetImage(relPath, img, card, placeholder);
}

async function fetchAndSetImage(relPath, imgElement, card, placeholder) {
    try {
        const base64Data = await invoke('decrypt_dat_file', { filePath: relPath });

        // 判断图片类型
        let mimeType = 'image/jpeg';
        if (base64Data.startsWith('/9j/')) mimeType = 'image/jpeg';
        else if (base64Data.startsWith('iVBORw0KGgo')) mimeType = 'image/png';
        else if (base64Data.startsWith('R0lGOD')) mimeType = 'image/gif';

        imgElement.src = `data:${mimeType};base64,${base64Data}`;

        imgElement.onload = () => {
            card.classList.remove('is-loading');
            imgElement.style.display = 'block';
            placeholder.style.display = 'none';
        };

        imgElement.onerror = () => {
            placeholder.textContent = '加载失败';
            card.classList.remove('is-loading');
        };
    } catch (error) {
        console.error('解密图片失败:', error);
        placeholder.textContent = '解密失败';
        card.classList.remove('is-loading');
    }
}

// --- 设置管理 ---
async function loadSettings() {
    try {
        const [xor, aes] = await invoke('get_keys');
        xorKeyInput.value = xor;
        aesKeyInput.value = aes;
    } catch (error) {
        console.error('加载设置失败:', error);
    }
}

async function saveSettings() {
    try {
        const xor = parseInt(xorKeyInput.value) || 0;
        const aes = aesKeyInput.value || '';

        await invoke('update_keys', { xor, aes });
        saveStatus.textContent = '保存成功!';
        saveStatus.style.color = 'green';

        setTimeout(() => {
            saveStatus.textContent = '';
        }, 2000);
    } catch (error) {
        console.error('保存设置失败:', error);
        saveStatus.textContent = '保存失败: ' + error;
        saveStatus.style.color = 'red';
    }
}

// --- 初始化 ---
window.addEventListener('DOMContentLoaded', () => {
    // 获取元素
    homeBtn = document.getElementById('home-btn');
    folderBtn = document.getElementById('folder-btn');
    settingsBtn = document.getElementById('settings-btn');
    layoutToggleBtn = document.getElementById('layout-toggle-btn');
    selectFolderBtn = document.getElementById('select-folder-btn');
    homeView = document.getElementById('home-view');
    folderView = document.getElementById('folder-view');
    settingsView = document.getElementById('settings-view');
    dirTree = document.getElementById('dir-tree');
    breadcrumb = document.getElementById('breadcrumb');
    imageGallery = document.getElementById('image-gallery');
    scrollContainer = document.querySelector('.gallery-scroll-container');
    currentFolderInfo = document.getElementById('current-folder-info');
    xorKeyInput = document.getElementById('xor-key-input');
    aesKeyInput = document.getElementById('aes-key-input');
    saveKeysBtn = document.getElementById('save-keys-btn');
    saveStatus = document.getElementById('save-status');
    layoutIconWaterfall = document.getElementById('layout-icon-waterfall');
    layoutIconGrid = document.getElementById('layout-icon-grid');

    // 事件监听
    homeBtn.addEventListener('click', () => switchView('home'));
    folderBtn.addEventListener('click', () => {
        if (currentRootDir) switchView('folder');
        else alert('请先在主页选择一个根目录！');
    });
    settingsBtn.addEventListener('click', () => switchView('settings'));
    layoutToggleBtn.addEventListener('click', toggleLayout);
    selectFolderBtn.addEventListener('click', selectFolder);
    saveKeysBtn.addEventListener('click', saveSettings);

    dirTree.addEventListener('click', (e) => {
        const clickedItem = e.target.closest('fluent-tree-item');
        if (clickedItem) selectTreeNode(clickedItem);
    });

    breadcrumb.addEventListener('click', (e) => {
        const item = e.target.closest('fluent-breadcrumb-item');
        if (item && item.dataset.path) {
            startImageLoading(item.dataset.path);
        }
    });

    window.addEventListener('resize', debounce(() => {
        if (allImagePaths.length > 0 && !folderView.classList.contains('hidden')) {
            const folderPath = breadcrumb.querySelector('fluent-breadcrumb-item:last-child')?.dataset.path;
            if (folderPath) startImageLoading(folderPath);
        }
    }, 250));

    // 初始视图
    switchView('home');
});
