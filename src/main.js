const { invoke } = window.__TAURI__.core;

// 图片数据缓存
const imageDataCache = new Map();

// 清理图片缓存
function clearImageCache() {
    // 释放所有 Blob URLs
    for (const blobUrl of imageDataCache.values()) {
        URL.revokeObjectURL(blobUrl);
    }
    imageDataCache.clear();

    // 通知后端清理缓存
    invoke('clear_image_cache').catch(err => {
        console.warn('清理后端缓存失败:', err);
    });
}

// --- 元素获取 ---
let homeBtn, folderBtn, settingsBtn, layoutToggleBtn;
let selectFolderBtn;
let homeView, folderView, settingsView;
let dirTree, breadcrumb, imageGallery, scrollContainer;
let currentFolderInfo;
let xorKeyInput, aesKeyInput, saveKeysBtn, saveStatus;
let layoutIconWaterfall, layoutIconGrid;
let sortSelect, hideThumbnailsCheckbox;

// --- 状态管理 ---
let currentRootDir = null;
let currentImageIndex = 0;
let columnElements = [];
let sentinelObserver = null;
let isLoading = false;
let currentLayout = 'grid'; // 'waterfall' 或 'grid'
let currentSortOrder = 'time-desc';
let hideThumbnails = false;
let currentPage = 0;
let hasMoreImages = true;
let totalImages = 0;
const PAGE_SIZE = 20;

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
        currentImageIndex = 0;
        currentPage = 0;
        hasMoreImages = true;
        totalImages = 0;
        imageGallery.innerHTML = '';
        columnElements = [];

        // 清除图片缓存
        clearImageCache();

        if (scrollContainer) {
            scrollContainer.scrollTop = 0;
        }

        // 设置布局
        setupLayout();

        // 加载初始图片
        await loadMoreImages(folderPath);

        // 检查是否需要继续加载以填满视口
        await ensureViewportFilled(folderPath);
    } catch (error) {
        console.error('加载图片失败:', error);
        imageGallery.innerHTML = '<p style="text-align:center;padding:20px;color:red;">加载失败: ' + error + '</p>';
    }
}

// 加载图片的 Blob URL
async function loadImageBlob(imageId, mimeType = 'image/jpeg') {
    // 检查缓存
    if (imageDataCache.has(imageId)) {
        return imageDataCache.get(imageId);
    }

    try {
        // 从后端获取图片二进制数据
        const imageData = await invoke('get_image_data', { imageId });
        const bytes = imageData?.data ?? [];
        const resolvedMime = imageData?.mime_type || mimeType || 'image/jpeg';

        // 创建 Blob 和 URL
        const blob = new Blob([new Uint8Array(bytes)], { type: resolvedMime });
        const blobUrl = URL.createObjectURL(blob);

        // 缓存 URL
        imageDataCache.set(imageId, blobUrl);

        return blobUrl;
    } catch (error) {
        console.error('加载图片失败:', imageId, error);
        return null;
    }
}

// 确保视口被填满
async function ensureViewportFilled(folderPath) {
    if (!hasMoreImages || isLoading) return;

    // 等待DOM更新和图片渲染
    await new Promise(resolve => setTimeout(resolve, 100));

    // 检查是否需要加载更多
    const needsMore = checkIfNeedsMoreContent();

    if (needsMore && hasMoreImages) {
        await loadMoreImages(folderPath);
        // 递归检查，直到填满或没有更多图片
        await ensureViewportFilled(folderPath);
    }
}

// 检查是否需要更多内容来填充视口
function checkIfNeedsMoreContent() {
    if (!scrollContainer) return false;

    if (currentLayout === 'grid') {
        // Grid模式：检查画廊高度是否小于容器高度
        return imageGallery.scrollHeight <= scrollContainer.clientHeight + 100;
    } else {
        // 瀑布流模式：检查最短列是否足够高
        if (columnElements.length === 0) return false;
        const shortestColumn = getShortestColumn();
        return shortestColumn.offsetHeight <= scrollContainer.clientHeight;
    }
}

function setupLayout() {
    imageGallery.innerHTML = '';
    columnElements = [];

    if (currentLayout === 'grid') {
        // 网格布局
        imageGallery.classList.add('grid-layout');
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

    const existingSentinel = scrollContainer.querySelector('#sentinel');
    if (existingSentinel) {
        existingSentinel.remove();
    }

    const sentinel = document.createElement('div');
    sentinel.id = 'sentinel';
    sentinel.style.height = '1px';
    scrollContainer.appendChild(sentinel);

    sentinelObserver = new IntersectionObserver((entries) => {
        if (entries[0].isIntersecting && !isLoading && hasMoreImages) {
            const folderPath = breadcrumb.querySelector('fluent-breadcrumb-item:last-child')?.dataset.path;
            if (folderPath) {
                loadMoreImages(folderPath);
            }
        }
    }, {
        root: scrollContainer,
        rootMargin: '500px',  // 提前500px开始加载
        threshold: 0.1
    });

    sentinelObserver.observe(sentinel);
}

function handleScrollLoad() {
    if (!scrollContainer || isLoading || !hasMoreImages) {
        return;
    }

    const remaining = scrollContainer.scrollHeight - scrollContainer.scrollTop - scrollContainer.clientHeight;
    if (remaining <= 300) {
        const folderPath = breadcrumb.querySelector('fluent-breadcrumb-item:last-child')?.dataset.path;
        if (folderPath) {
            loadMoreImages(folderPath);
        }
    }
}

async function loadMoreImages(folderPath) {
    if (isLoading || !hasMoreImages) return;

    isLoading = true;

    try {
        // 解析排序参数
        const [sortBy, sortOrder] = currentSortOrder.split('-');

        // 调用后端批量获取接口
        const batch = await invoke('get_images_batch', {
            folderPath,
            page: currentPage,
            pageSize: PAGE_SIZE,
            sortBy,
            sortOrder,
            hideThumbnails
        });

        totalImages = batch.total;
        hasMoreImages = batch.has_more;
        currentPage++;

        if (batch.images.length === 0 && currentPage === 1) {
            imageGallery.innerHTML = '<p style="text-align:center;padding:20px;">该文件夹中没有图片</p>';
            return;
        }

        // 添加图片到界面
        for (const imageData of batch.images) {
            if (currentLayout === 'grid') {
                addImageToGallery(imageData);
            } else {
                const shortestColumn = getShortestColumn();
                addImageToColumn(shortestColumn, imageData);
            }
        }
    } catch (error) {
        console.error('加载图片失败:', error);
        if (currentPage === 1) {
            imageGallery.innerHTML = '<p style="text-align:center;padding:20px;color:red;">加载失败: ' + error + '</p>';
        }
    } finally {
        isLoading = false;
    }
}

function getShortestColumn() {
    return columnElements.reduce((shortest, current) =>
        current.offsetHeight < shortest.offsetHeight ? current : shortest
    );
}

function addImageToGallery(imageData) {
    const card = document.createElement('fluent-card');
    card.className = 'image-card';

    const img = document.createElement('img');
    img.alt = imageData.name;
    img.dataset.imageId = imageData.image_id;

    // 使用占位符
    img.src = 'data:image/svg+xml,%3Csvg xmlns="http://www.w3.org/2000/svg" width="200" height="200"%3E%3Crect fill="%23ddd" width="200" height="200"/%3E%3C/svg%3E';

    // 异步加载图片
    loadImageBlob(imageData.image_id, imageData.mime_type).then(blobUrl => {
        if (blobUrl) {
            img.src = blobUrl;
        }
    });

    const caption = document.createElement('div');
    caption.className = 'caption';
    caption.textContent = imageData.name;

    card.appendChild(img);
    card.appendChild(caption);
    imageGallery.appendChild(card);
}

function addImageToColumn(column, imageData) {
    const card = document.createElement('fluent-card');
    card.className = 'image-card';

    const img = document.createElement('img');
    img.alt = imageData.name;
    img.dataset.imageId = imageData.image_id;

    // 使用占位符
    img.src = 'data:image/svg+xml,%3Csvg xmlns="http://www.w3.org/2000/svg" width="200" height="200"%3E%3Crect fill="%23ddd" width="200" height="200"/%3E%3C/svg%3E';

    // 异步加载图片
    loadImageBlob(imageData.image_id, imageData.mime_type).then(blobUrl => {
        if (blobUrl) {
            img.src = blobUrl;
        }
    });

    const caption = document.createElement('div');
    caption.className = 'caption';
    caption.textContent = imageData.name;

    card.appendChild(img);
    card.appendChild(caption);
    column.appendChild(card);
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
    sortSelect = document.getElementById('sort-select');
    hideThumbnailsCheckbox = document.getElementById('hide-thumbnails-checkbox');

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

    // 排序和筛选事件
    sortSelect.addEventListener('change', (e) => {
        currentSortOrder = e.target.value;
        const folderPath = breadcrumb.querySelector('fluent-breadcrumb-item:last-child')?.dataset.path;
        if (folderPath) {
            startImageLoading(folderPath);
        }
    });

    hideThumbnailsCheckbox.addEventListener('change', (e) => {
        hideThumbnails = e.target.checked;
        const folderPath = breadcrumb.querySelector('fluent-breadcrumb-item:last-child')?.dataset.path;
        if (folderPath) {
            startImageLoading(folderPath);
        }
    });

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

    scrollContainer.addEventListener('scroll', handleScrollLoad);

    window.addEventListener('resize', debounce(() => {
        if (currentRootDir && !folderView.classList.contains('hidden')) {
            const folderPath = breadcrumb.querySelector('fluent-breadcrumb-item:last-child')?.dataset.path;
            if (folderPath) startImageLoading(folderPath);
        }
    }, 250));

    // 页面卸载时清理缓存
    window.addEventListener('beforeunload', () => {
        clearImageCache();
    });

    // 初始视图
    switchView('home');
});
