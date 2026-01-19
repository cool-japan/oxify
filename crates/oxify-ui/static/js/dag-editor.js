/**
 * DAG Editor - Interactive workflow visual editor
 *
 * Features:
 * - Drag and drop nodes from palette
 * - Connect nodes by dragging ports
 * - Select, move, and delete nodes
 * - Zoom and pan canvas
 * - Undo/redo support
 * - Copy/paste nodes
 * - Keyboard shortcuts
 * - Workflow validation
 */

class DAGEditor {
    constructor(containerId, options = {}) {
        this.container = document.getElementById(containerId);
        this.svg = null;
        this.nodes = [];
        this.edges = [];
        this.selectedNode = null;
        this.selectedEdge = null;
        this.selectedNodes = []; // Multi-select support
        this.draggedNode = null;
        this.connectingFrom = null;
        this.zoom = 1;
        this.panX = 0;
        this.panY = 0;
        this.history = [];
        this.historyIndex = -1;
        this.isPanning = false;
        this.lastMousePos = { x: 0, y: 0 };
        this.clipboard = null;

        // Configuration
        this.nodeWidth = 160;
        this.nodeHeight = 60;
        this.portRadius = 8;
        this.gridSize = 20;
        this.snapToGrid = options.snapToGrid !== false;

        // Node type colors
        this.nodeColors = {
            start: { fill: '#10b981', stroke: '#047857', text: '#ffffff' },
            end: { fill: '#ef4444', stroke: '#b91c1c', text: '#ffffff' },
            llm: { fill: '#3b82f6', stroke: '#1d4ed8', text: '#ffffff' },
            retriever: { fill: '#8b5cf6', stroke: '#6d28d9', text: '#ffffff' },
            code: { fill: '#22c55e', stroke: '#15803d', text: '#ffffff' },
            conditional: { fill: '#eab308', stroke: '#a16207', text: '#000000' },
            tool: { fill: '#f97316', stroke: '#c2410c', text: '#ffffff' },
            loop: { fill: '#06b6d4', stroke: '#0891b2', text: '#ffffff' },
            parallel: { fill: '#ec4899', stroke: '#be185d', text: '#ffffff' },
            switch: { fill: '#84cc16', stroke: '#65a30d', text: '#000000' },
            approval: { fill: '#f59e0b', stroke: '#d97706', text: '#000000' },
            form: { fill: '#14b8a6', stroke: '#0d9488', text: '#ffffff' },
            vision: { fill: '#a855f7', stroke: '#7e22ce', text: '#ffffff' },
        };

        // Execution status colors
        this.statusColors = {
            pending: { stroke: '#9ca3af', badge: '#9ca3af' },
            running: { stroke: '#3b82f6', badge: '#3b82f6' },
            completed: { stroke: '#22c55e', badge: '#22c55e' },
            failed: { stroke: '#ef4444', badge: '#ef4444' },
            cancelled: { stroke: '#f97316', badge: '#f97316' },
            skipped: { stroke: '#6b7280', badge: '#6b7280' },
        };

        // Node execution statuses (nodeId -> status)
        this.nodeStatuses = {};

        // Callbacks
        this.onNodeSelect = options.onNodeSelect || (() => {});
        this.onNodeDeselect = options.onNodeDeselect || (() => {});
        this.onChange = options.onChange || (() => {});
        this.onValidation = options.onValidation || (() => {});

        this.init();
    }

    init() {
        // Create SVG element
        this.svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
        this.svg.setAttribute('class', 'dag-editor-canvas');
        this.svg.style.width = '100%';
        this.svg.style.height = '100%';
        this.svg.setAttribute('tabindex', '0'); // Make focusable for keyboard events
        this.container.appendChild(this.svg);

        // Create groups for layering
        this.gridGroup = this.createGroup('grid-group');
        this.edgesGroup = this.createGroup('edges-group');
        this.nodesGroup = this.createGroup('nodes-group');
        this.tempGroup = this.createGroup('temp-group');

        // Add defs for markers
        this.createDefs();

        // Draw grid
        this.drawGrid();

        // Set up event listeners
        this.setupEventListeners();

        // Apply initial transform
        this.updateTransform();
    }

    createGroup(id) {
        const group = document.createElementNS('http://www.w3.org/2000/svg', 'g');
        group.setAttribute('id', id);
        this.svg.appendChild(group);
        return group;
    }

    createDefs() {
        const defs = document.createElementNS('http://www.w3.org/2000/svg', 'defs');

        // Arrow marker for edges
        const marker = document.createElementNS('http://www.w3.org/2000/svg', 'marker');
        marker.setAttribute('id', 'arrow');
        marker.setAttribute('markerWidth', '10');
        marker.setAttribute('markerHeight', '10');
        marker.setAttribute('refX', '9');
        marker.setAttribute('refY', '3');
        marker.setAttribute('orient', 'auto');
        marker.setAttribute('markerUnits', 'strokeWidth');

        const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
        path.setAttribute('d', 'M0,0 L0,6 L9,3 z');
        path.setAttribute('fill', '#6b7280');
        marker.appendChild(path);
        defs.appendChild(marker);

        // Selected arrow marker
        const selectedMarker = marker.cloneNode(true);
        selectedMarker.setAttribute('id', 'arrow-selected');
        selectedMarker.querySelector('path').setAttribute('fill', '#3b82f6');
        defs.appendChild(selectedMarker);

        // Error arrow marker
        const errorMarker = marker.cloneNode(true);
        errorMarker.setAttribute('id', 'arrow-error');
        errorMarker.querySelector('path').setAttribute('fill', '#ef4444');
        defs.appendChild(errorMarker);

        // Running arrow marker (animated)
        const runningMarker = marker.cloneNode(true);
        runningMarker.setAttribute('id', 'arrow-running');
        runningMarker.querySelector('path').setAttribute('fill', '#3b82f6');
        defs.appendChild(runningMarker);

        // Completed arrow marker
        const completedMarker = marker.cloneNode(true);
        completedMarker.setAttribute('id', 'arrow-completed');
        completedMarker.querySelector('path').setAttribute('fill', '#22c55e');
        defs.appendChild(completedMarker);

        // Pulsing animation for running nodes
        const pulseAnim = document.createElementNS('http://www.w3.org/2000/svg', 'style');
        pulseAnim.textContent = `
            @keyframes pulse {
                0%, 100% { opacity: 1; }
                50% { opacity: 0.5; }
            }
            @keyframes spin {
                from { transform: rotate(0deg); }
                to { transform: rotate(360deg); }
            }
            .status-running {
                animation: pulse 1.5s ease-in-out infinite;
            }
            .status-running-spinner {
                animation: spin 1s linear infinite;
                transform-origin: center;
            }
        `;
        defs.appendChild(pulseAnim);

        this.svg.appendChild(defs);
    }

    drawGrid() {
        const pattern = document.createElementNS('http://www.w3.org/2000/svg', 'pattern');
        pattern.setAttribute('id', 'grid');
        pattern.setAttribute('width', this.gridSize);
        pattern.setAttribute('height', this.gridSize);
        pattern.setAttribute('patternUnits', 'userSpaceOnUse');

        const circle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
        circle.setAttribute('cx', '1');
        circle.setAttribute('cy', '1');
        circle.setAttribute('r', '1');
        circle.setAttribute('fill', '#e5e7eb');
        circle.setAttribute('class', 'grid-dot');
        pattern.appendChild(circle);

        this.svg.querySelector('defs').appendChild(pattern);

        const rect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
        rect.setAttribute('width', '10000');
        rect.setAttribute('height', '10000');
        rect.setAttribute('x', '-5000');
        rect.setAttribute('y', '-5000');
        rect.setAttribute('fill', 'url(#grid)');
        this.gridGroup.appendChild(rect);
    }

    setupEventListeners() {
        // Mouse events for panning
        this.svg.addEventListener('mousedown', (e) => this.onMouseDown(e));
        this.svg.addEventListener('mousemove', (e) => this.onMouseMove(e));
        this.svg.addEventListener('mouseup', (e) => this.onMouseUp(e));
        this.svg.addEventListener('mouseleave', (e) => this.onMouseUp(e));
        this.svg.addEventListener('dblclick', (e) => this.onDoubleClick(e));

        // Wheel for zooming
        this.svg.addEventListener('wheel', (e) => this.onWheel(e));

        // Keyboard events on the SVG element
        this.svg.addEventListener('keydown', (e) => this.onKeyDown(e));

        // Also listen on document for keyboard shortcuts when focused
        document.addEventListener('keydown', (e) => {
            if (document.activeElement === this.svg || this.container.contains(document.activeElement)) {
                this.onKeyDown(e);
            }
        });

        // Prevent context menu on right click
        this.svg.addEventListener('contextmenu', (e) => e.preventDefault());

        // Focus on click
        this.svg.addEventListener('click', () => this.svg.focus());
    }

    onMouseDown(e) {
        const rect = this.svg.getBoundingClientRect();
        const x = (e.clientX - rect.left) / this.zoom - this.panX / this.zoom;
        const y = (e.clientY - rect.top) / this.zoom - this.panY / this.zoom;

        // Check if clicking on an edge
        const edge = this.findEdgeAt(x, y);
        if (edge && e.button === 0) {
            this.selectEdge(edge);
            return;
        }

        // Check if clicking on a node
        const node = this.findNodeAt(x, y);
        if (node) {
            // Check if clicking on output port
            if (this.isOnOutputPort(node, x, y)) {
                this.startConnection(node);
                return;
            }

            this.selectNode(node);
            this.draggedNode = node;
            this.dragOffset = { x: x - node.x, y: y - node.y };
        } else {
            this.deselectAll();

            // Start panning with middle mouse or space+left click
            if (e.button === 1 || (e.button === 0 && e.shiftKey)) {
                this.isPanning = true;
                this.lastMousePos = { x: e.clientX, y: e.clientY };
            }
        }
    }

    onMouseMove(e) {
        const rect = this.svg.getBoundingClientRect();
        const x = (e.clientX - rect.left - this.panX) / this.zoom;
        const y = (e.clientY - rect.top - this.panY) / this.zoom;

        if (this.draggedNode) {
            let newX = x - this.dragOffset.x;
            let newY = y - this.dragOffset.y;

            if (this.snapToGrid) {
                newX = Math.round(newX / this.gridSize) * this.gridSize;
                newY = Math.round(newY / this.gridSize) * this.gridSize;
            }

            this.draggedNode.x = newX;
            this.draggedNode.y = newY;
            this.updateNodePosition(this.draggedNode);
            this.updateEdges();
        } else if (this.connectingFrom) {
            this.updateTempEdge(x, y);
        } else if (this.isPanning) {
            const dx = e.clientX - this.lastMousePos.x;
            const dy = e.clientY - this.lastMousePos.y;
            this.panX += dx;
            this.panY += dy;
            this.lastMousePos = { x: e.clientX, y: e.clientY };
            this.updateTransform();
        }
    }

    onMouseUp(e) {
        const rect = this.svg.getBoundingClientRect();
        const x = (e.clientX - rect.left - this.panX) / this.zoom;
        const y = (e.clientY - rect.top - this.panY) / this.zoom;

        if (this.draggedNode) {
            this.saveState();
            this.draggedNode = null;
            this.onChange(this.getData());
        }

        if (this.connectingFrom) {
            const targetNode = this.findNodeAt(x, y);
            if (targetNode && targetNode !== this.connectingFrom && this.isOnInputPort(targetNode, x, y)) {
                this.addEdge(this.connectingFrom.id, targetNode.id);
            }
            this.clearTempEdge();
            this.connectingFrom = null;
        }

        this.isPanning = false;
    }

    onDoubleClick(e) {
        const rect = this.svg.getBoundingClientRect();
        const x = (e.clientX - rect.left - this.panX) / this.zoom;
        const y = (e.clientY - rect.top - this.panY) / this.zoom;

        const node = this.findNodeAt(x, y);
        if (node) {
            // Trigger node edit (could open a modal or inline edit)
            this.onNodeSelect(node);
            // Focus on name input in properties panel
            setTimeout(() => {
                const nameInput = document.querySelector('#node-config-form input[type="text"]');
                if (nameInput) nameInput.focus();
            }, 100);
        }
    }

    onWheel(e) {
        e.preventDefault();
        const delta = e.deltaY > 0 ? 0.9 : 1.1;
        const newZoom = Math.min(Math.max(this.zoom * delta, 0.25), 2);

        // Zoom towards mouse position
        const rect = this.svg.getBoundingClientRect();
        const mouseX = e.clientX - rect.left;
        const mouseY = e.clientY - rect.top;

        this.panX = mouseX - (mouseX - this.panX) * (newZoom / this.zoom);
        this.panY = mouseY - (mouseY - this.panY) * (newZoom / this.zoom);
        this.zoom = newZoom;

        this.updateTransform();
    }

    onKeyDown(e) {
        // Don't capture if typing in input
        if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') {
            return;
        }

        const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
        const cmdOrCtrl = isMac ? e.metaKey : e.ctrlKey;

        // Delete selected node/edge
        if ((e.key === 'Delete' || e.key === 'Backspace') && (this.selectedNode || this.selectedEdge)) {
            e.preventDefault();
            if (this.selectedNode) {
                this.deleteNode(this.selectedNode.id);
            } else if (this.selectedEdge) {
                this.deleteEdge(this.selectedEdge.from, this.selectedEdge.to);
            }
        }

        // Undo: Cmd/Ctrl + Z
        if (cmdOrCtrl && e.key === 'z' && !e.shiftKey) {
            e.preventDefault();
            this.undo();
        }

        // Redo: Cmd/Ctrl + Shift + Z or Cmd/Ctrl + Y
        if (cmdOrCtrl && ((e.key === 'z' && e.shiftKey) || e.key === 'y')) {
            e.preventDefault();
            this.redo();
        }

        // Copy: Cmd/Ctrl + C
        if (cmdOrCtrl && e.key === 'c' && this.selectedNode) {
            e.preventDefault();
            this.copyNode();
        }

        // Paste: Cmd/Ctrl + V
        if (cmdOrCtrl && e.key === 'v' && this.clipboard) {
            e.preventDefault();
            this.pasteNode();
        }

        // Duplicate: Cmd/Ctrl + D
        if (cmdOrCtrl && e.key === 'd' && this.selectedNode) {
            e.preventDefault();
            this.duplicateNode();
        }

        // Select All: Cmd/Ctrl + A
        if (cmdOrCtrl && e.key === 'a') {
            e.preventDefault();
            this.selectAllNodes();
        }

        // Escape: Deselect
        if (e.key === 'Escape') {
            this.deselectAll();
            this.clearTempEdge();
            this.connectingFrom = null;
        }

        // Arrow keys: Move selected node
        if (this.selectedNode && ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'].includes(e.key)) {
            e.preventDefault();
            const step = e.shiftKey ? this.gridSize * 2 : this.gridSize;
            switch (e.key) {
                case 'ArrowUp': this.selectedNode.y -= step; break;
                case 'ArrowDown': this.selectedNode.y += step; break;
                case 'ArrowLeft': this.selectedNode.x -= step; break;
                case 'ArrowRight': this.selectedNode.x += step; break;
            }
            this.updateNodePosition(this.selectedNode);
            this.updateEdges();
            this.saveState();
            this.onChange(this.getData());
        }

        // + / -: Zoom
        if (e.key === '=' || e.key === '+') {
            e.preventDefault();
            this.zoomIn();
        }
        if (e.key === '-') {
            e.preventDefault();
            this.zoomOut();
        }

        // 0: Reset zoom
        if (e.key === '0' && cmdOrCtrl) {
            e.preventDefault();
            this.resetZoom();
        }

        // F: Fit to content
        if (e.key === 'f' && !cmdOrCtrl) {
            e.preventDefault();
            this.fitToContent();
        }
    }

    // Copy node to clipboard
    copyNode() {
        if (!this.selectedNode) return;
        this.clipboard = JSON.parse(JSON.stringify(this.selectedNode));
    }

    // Paste node from clipboard
    pasteNode() {
        if (!this.clipboard) return;
        const newNode = {
            ...this.clipboard,
            id: this.generateId(),
            x: this.clipboard.x + 40,
            y: this.clipboard.y + 40,
            name: this.clipboard.name + ' (copy)'
        };
        this.nodes.push(newNode);
        this.renderNode(newNode);
        this.selectNode(newNode);
        this.saveState();
        this.onChange(this.getData());
    }

    // Duplicate node
    duplicateNode() {
        if (!this.selectedNode) return;
        const newNode = {
            ...JSON.parse(JSON.stringify(this.selectedNode)),
            id: this.generateId(),
            x: this.selectedNode.x + 40,
            y: this.selectedNode.y + 40,
            name: this.selectedNode.name + ' (copy)'
        };
        this.nodes.push(newNode);
        this.renderNode(newNode);
        this.selectNode(newNode);
        this.saveState();
        this.onChange(this.getData());
    }

    // Select all nodes
    selectAllNodes() {
        this.selectedNodes = [...this.nodes];
        this.nodes.forEach(node => {
            const group = this.nodesGroup.querySelector(`#node-${node.id}`);
            if (group) {
                group.classList.add('selected');
                const rect = group.querySelector('.node-bg');
                rect.setAttribute('stroke', '#3b82f6');
                rect.setAttribute('stroke-width', '3');
            }
        });
        if (this.nodes.length > 0) {
            this.selectedNode = this.nodes[0];
            this.onNodeSelect(this.selectedNode);
        }
    }

    findNodeAt(x, y) {
        return this.nodes.find(node =>
            x >= node.x && x <= node.x + this.nodeWidth &&
            y >= node.y && y <= node.y + this.nodeHeight
        );
    }

    findEdgeAt(x, y) {
        // Simple proximity check for edges
        for (const edge of this.edges) {
            const fromNode = this.nodes.find(n => n.id === edge.from);
            const toNode = this.nodes.find(n => n.id === edge.to);
            if (!fromNode || !toNode) continue;

            const x1 = fromNode.x + this.nodeWidth;
            const y1 = fromNode.y + this.nodeHeight / 2;
            const x2 = toNode.x;
            const y2 = toNode.y + this.nodeHeight / 2;

            // Check if point is near the line (simplified)
            const dist = this.pointToLineDistance(x, y, x1, y1, x2, y2);
            if (dist < 10) {
                return edge;
            }
        }
        return null;
    }

    pointToLineDistance(px, py, x1, y1, x2, y2) {
        const A = px - x1;
        const B = py - y1;
        const C = x2 - x1;
        const D = y2 - y1;
        const dot = A * C + B * D;
        const lenSq = C * C + D * D;
        let param = -1;
        if (lenSq !== 0) param = dot / lenSq;

        let xx, yy;
        if (param < 0) {
            xx = x1;
            yy = y1;
        } else if (param > 1) {
            xx = x2;
            yy = y2;
        } else {
            xx = x1 + param * C;
            yy = y1 + param * D;
        }

        const dx = px - xx;
        const dy = py - yy;
        return Math.sqrt(dx * dx + dy * dy);
    }

    selectEdge(edge) {
        this.deselectAll();
        this.selectedEdge = edge;

        // Highlight the edge
        const edgePath = this.edgesGroup.querySelector(`[data-from="${edge.from}"][data-to="${edge.to}"]`);
        if (edgePath) {
            edgePath.setAttribute('stroke', '#3b82f6');
            edgePath.setAttribute('stroke-width', '3');
            edgePath.setAttribute('marker-end', 'url(#arrow-selected)');
        }
    }

    deleteEdge(fromId, toId) {
        this.edges = this.edges.filter(e => !(e.from === fromId && e.to === toId));
        this.renderEdges();
        this.selectedEdge = null;
        this.saveState();
        this.onChange(this.getData());
    }

    isOnOutputPort(node, x, y) {
        const portX = node.x + this.nodeWidth;
        const portY = node.y + this.nodeHeight / 2;
        return Math.hypot(x - portX, y - portY) <= this.portRadius * 2;
    }

    isOnInputPort(node, x, y) {
        const portX = node.x;
        const portY = node.y + this.nodeHeight / 2;
        return Math.hypot(x - portX, y - portY) <= this.portRadius * 2;
    }

    updateTransform() {
        const transform = `translate(${this.panX}, ${this.panY}) scale(${this.zoom})`;
        this.gridGroup.setAttribute('transform', transform);
        this.edgesGroup.setAttribute('transform', transform);
        this.nodesGroup.setAttribute('transform', transform);
        this.tempGroup.setAttribute('transform', transform);
    }

    // Node operations
    addNode(type, x, y, config = {}) {
        const id = config.id || this.generateId();
        const name = config.name || this.getDefaultNodeName(type);

        if (this.snapToGrid) {
            x = Math.round(x / this.gridSize) * this.gridSize;
            y = Math.round(y / this.gridSize) * this.gridSize;
        }

        const node = { id, type, name, x, y, config: config.config || {} };
        this.nodes.push(node);
        this.renderNode(node);
        this.saveState();
        this.onChange(this.getData());
        return node;
    }

    renderNode(node) {
        const group = document.createElementNS('http://www.w3.org/2000/svg', 'g');
        group.setAttribute('id', `node-${node.id}`);
        group.setAttribute('class', 'dag-node');
        group.setAttribute('data-node-id', node.id);

        const colors = this.nodeColors[node.type] || { fill: '#6b7280', stroke: '#4b5563', text: '#ffffff' };

        // Node background
        const rect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
        rect.setAttribute('x', node.x);
        rect.setAttribute('y', node.y);
        rect.setAttribute('width', this.nodeWidth);
        rect.setAttribute('height', this.nodeHeight);
        rect.setAttribute('rx', '8');
        rect.setAttribute('ry', '8');
        rect.setAttribute('fill', colors.fill);
        rect.setAttribute('stroke', colors.stroke);
        rect.setAttribute('stroke-width', '2');
        rect.setAttribute('class', 'node-bg');
        group.appendChild(rect);

        // Node type icon (small box at top-left)
        const iconRect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
        iconRect.setAttribute('x', node.x + 8);
        iconRect.setAttribute('y', node.y + 8);
        iconRect.setAttribute('width', '24');
        iconRect.setAttribute('height', '16');
        iconRect.setAttribute('rx', '3');
        iconRect.setAttribute('fill', 'rgba(255,255,255,0.3)');
        group.appendChild(iconRect);

        // Node type label
        const typeLabel = document.createElementNS('http://www.w3.org/2000/svg', 'text');
        typeLabel.setAttribute('x', node.x + 20);
        typeLabel.setAttribute('y', node.y + 20);
        typeLabel.setAttribute('text-anchor', 'middle');
        typeLabel.setAttribute('font-size', '8');
        typeLabel.setAttribute('font-weight', 'bold');
        typeLabel.setAttribute('fill', colors.text);
        typeLabel.textContent = node.type.toUpperCase().slice(0, 3);
        group.appendChild(typeLabel);

        // Node name
        const text = document.createElementNS('http://www.w3.org/2000/svg', 'text');
        text.setAttribute('x', node.x + this.nodeWidth / 2);
        text.setAttribute('y', node.y + this.nodeHeight / 2 + 8);
        text.setAttribute('text-anchor', 'middle');
        text.setAttribute('font-size', '12');
        text.setAttribute('font-weight', '500');
        text.setAttribute('fill', colors.text);
        text.textContent = node.name.length > 18 ? node.name.slice(0, 18) + '...' : node.name;
        group.appendChild(text);

        // Input port (left side, except for start nodes)
        if (node.type !== 'start') {
            const inputPort = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
            inputPort.setAttribute('cx', node.x);
            inputPort.setAttribute('cy', node.y + this.nodeHeight / 2);
            inputPort.setAttribute('r', this.portRadius);
            inputPort.setAttribute('fill', '#ffffff');
            inputPort.setAttribute('stroke', colors.stroke);
            inputPort.setAttribute('stroke-width', '2');
            inputPort.setAttribute('class', 'input-port');
            group.appendChild(inputPort);
        }

        // Output port (right side, except for end nodes)
        if (node.type !== 'end') {
            const outputPort = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
            outputPort.setAttribute('cx', node.x + this.nodeWidth);
            outputPort.setAttribute('cy', node.y + this.nodeHeight / 2);
            outputPort.setAttribute('r', this.portRadius);
            outputPort.setAttribute('fill', '#ffffff');
            outputPort.setAttribute('stroke', colors.stroke);
            outputPort.setAttribute('stroke-width', '2');
            outputPort.setAttribute('class', 'output-port');
            group.appendChild(outputPort);
        }

        this.nodesGroup.appendChild(group);
    }

    updateNodePosition(node) {
        const group = this.nodesGroup.querySelector(`#node-${node.id}`);
        if (!group) return;

        // Update all elements' positions
        const rect = group.querySelector('.node-bg');
        rect.setAttribute('x', node.x);
        rect.setAttribute('y', node.y);

        const iconRect = group.querySelectorAll('rect')[1];
        if (iconRect) {
            iconRect.setAttribute('x', node.x + 8);
            iconRect.setAttribute('y', node.y + 8);
        }

        const texts = group.querySelectorAll('text');
        if (texts[0]) {
            texts[0].setAttribute('x', node.x + 20);
            texts[0].setAttribute('y', node.y + 20);
        }
        if (texts[1]) {
            texts[1].setAttribute('x', node.x + this.nodeWidth / 2);
            texts[1].setAttribute('y', node.y + this.nodeHeight / 2 + 8);
        }

        const inputPort = group.querySelector('.input-port');
        if (inputPort) {
            inputPort.setAttribute('cx', node.x);
            inputPort.setAttribute('cy', node.y + this.nodeHeight / 2);
        }

        const outputPort = group.querySelector('.output-port');
        if (outputPort) {
            outputPort.setAttribute('cx', node.x + this.nodeWidth);
            outputPort.setAttribute('cy', node.y + this.nodeHeight / 2);
        }
    }

    selectNode(node) {
        this.deselectAll();
        this.selectedNode = node;

        const group = this.nodesGroup.querySelector(`#node-${node.id}`);
        if (group) {
            group.classList.add('selected');
            const rect = group.querySelector('.node-bg');
            rect.setAttribute('stroke', '#3b82f6');
            rect.setAttribute('stroke-width', '3');
        }

        this.onNodeSelect(node);
    }

    deselectAll() {
        if (this.selectedNode) {
            const group = this.nodesGroup.querySelector(`#node-${this.selectedNode.id}`);
            if (group) {
                group.classList.remove('selected');
                const rect = group.querySelector('.node-bg');
                const colors = this.nodeColors[this.selectedNode.type] || { stroke: '#4b5563' };
                rect.setAttribute('stroke', colors.stroke);
                rect.setAttribute('stroke-width', '2');
            }
            this.onNodeDeselect();
        }
        this.selectedNode = null;

        // Deselect edge
        if (this.selectedEdge) {
            this.renderEdges(); // Re-render to clear selection
            this.selectedEdge = null;
        }

        // Clear multi-select
        this.selectedNodes = [];
    }

    deleteNode(nodeId) {
        const node = this.nodes.find(n => n.id === nodeId);
        if (!node) return;

        // Remove associated edges
        this.edges = this.edges.filter(e => e.from !== nodeId && e.to !== nodeId);

        // Remove node
        this.nodes = this.nodes.filter(n => n.id !== nodeId);

        // Remove from DOM
        const group = this.nodesGroup.querySelector(`#node-${nodeId}`);
        if (group) group.remove();

        this.renderEdges();
        this.deselectAll();
        this.saveState();
        this.onChange(this.getData());
    }

    // Edge operations
    addEdge(fromId, toId) {
        // Check if edge already exists
        if (this.edges.some(e => e.from === fromId && e.to === toId)) {
            return;
        }

        // Check for self-loops
        if (fromId === toId) return;

        // Check for reverse edge (would create immediate cycle)
        if (this.edges.some(e => e.from === toId && e.to === fromId)) {
            console.warn('Cannot create edge: would create immediate cycle');
            return;
        }

        this.edges.push({ from: fromId, to: toId });
        this.renderEdges();
        this.saveState();
        this.onChange(this.getData());
    }

    renderEdges() {
        // Clear existing edges
        this.edgesGroup.innerHTML = '';

        this.edges.forEach(edge => {
            const fromNode = this.nodes.find(n => n.id === edge.from);
            const toNode = this.nodes.find(n => n.id === edge.to);

            if (!fromNode || !toNode) return;

            const line = this.createEdgePath(fromNode, toNode);
            line.setAttribute('data-from', edge.from);
            line.setAttribute('data-to', edge.to);
            this.edgesGroup.appendChild(line);
        });
    }

    createEdgePath(fromNode, toNode) {
        const x1 = fromNode.x + this.nodeWidth;
        const y1 = fromNode.y + this.nodeHeight / 2;
        const x2 = toNode.x;
        const y2 = toNode.y + this.nodeHeight / 2;

        // Create a bezier curve
        const dx = Math.max((x2 - x1) / 2, 50);
        const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
        path.setAttribute('d', `M${x1},${y1} C${x1 + dx},${y1} ${x2 - dx},${y2} ${x2},${y2}`);
        path.setAttribute('stroke', '#6b7280');
        path.setAttribute('stroke-width', '2');
        path.setAttribute('fill', 'none');
        path.setAttribute('marker-end', 'url(#arrow)');
        path.setAttribute('class', 'edge');

        return path;
    }

    updateEdges() {
        this.renderEdges();
    }

    startConnection(node) {
        this.connectingFrom = node;
    }

    updateTempEdge(x, y) {
        this.clearTempEdge();

        if (!this.connectingFrom) return;

        const x1 = this.connectingFrom.x + this.nodeWidth;
        const y1 = this.connectingFrom.y + this.nodeHeight / 2;

        const dx = Math.max((x - x1) / 2, 30);
        const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
        path.setAttribute('d', `M${x1},${y1} C${x1 + dx},${y1} ${x - dx},${y} ${x},${y}`);
        path.setAttribute('stroke', '#3b82f6');
        path.setAttribute('stroke-width', '2');
        path.setAttribute('stroke-dasharray', '5,5');
        path.setAttribute('fill', 'none');
        path.setAttribute('id', 'temp-edge');

        this.tempGroup.appendChild(path);
    }

    clearTempEdge() {
        const temp = this.tempGroup.querySelector('#temp-edge');
        if (temp) temp.remove();
    }

    // Utility methods
    generateId() {
        return 'node_' + Math.random().toString(36).substr(2, 9);
    }

    getDefaultNodeName(type) {
        const names = {
            start: 'Start',
            end: 'End',
            llm: 'LLM Node',
            retriever: 'Retriever',
            code: 'Code',
            conditional: 'Condition',
            tool: 'Tool',
            loop: 'Loop',
            parallel: 'Parallel',
            switch: 'Switch',
            approval: 'Approval',
            form: 'Form Input',
            vision: 'Vision',
        };
        return names[type] || 'Node';
    }

    // State management
    saveState() {
        const state = JSON.stringify({ nodes: this.nodes, edges: this.edges });

        // Remove any states after current index
        this.history = this.history.slice(0, this.historyIndex + 1);
        this.history.push(state);
        this.historyIndex = this.history.length - 1;

        // Limit history size
        if (this.history.length > 50) {
            this.history.shift();
            this.historyIndex--;
        }
    }

    undo() {
        if (this.historyIndex > 0) {
            this.historyIndex--;
            this.restoreState(this.history[this.historyIndex]);
        }
    }

    redo() {
        if (this.historyIndex < this.history.length - 1) {
            this.historyIndex++;
            this.restoreState(this.history[this.historyIndex]);
        }
    }

    restoreState(stateJson) {
        const state = JSON.parse(stateJson);
        this.nodes = state.nodes;
        this.edges = state.edges;
        this.render();
        this.deselectAll();
        this.onChange(this.getData());
    }

    canUndo() {
        return this.historyIndex > 0;
    }

    canRedo() {
        return this.historyIndex < this.history.length - 1;
    }

    // Render all
    render() {
        this.nodesGroup.innerHTML = '';
        this.edgesGroup.innerHTML = '';

        this.nodes.forEach(node => this.renderNode(node));
        this.renderEdges();
    }

    // Zoom controls
    zoomIn() {
        this.zoom = Math.min(2, this.zoom * 1.2);
        this.updateTransform();
    }

    zoomOut() {
        this.zoom = Math.max(0.25, this.zoom / 1.2);
        this.updateTransform();
    }

    resetZoom() {
        this.zoom = 1;
        this.panX = 0;
        this.panY = 0;
        this.updateTransform();
    }

    fitToContent() {
        if (this.nodes.length === 0) return;

        const minX = Math.min(...this.nodes.map(n => n.x));
        const maxX = Math.max(...this.nodes.map(n => n.x + this.nodeWidth));
        const minY = Math.min(...this.nodes.map(n => n.y));
        const maxY = Math.max(...this.nodes.map(n => n.y + this.nodeHeight));

        const contentWidth = maxX - minX;
        const contentHeight = maxY - minY;

        const rect = this.svg.getBoundingClientRect();
        const scaleX = (rect.width - 100) / contentWidth;
        const scaleY = (rect.height - 100) / contentHeight;

        this.zoom = Math.min(Math.max(Math.min(scaleX, scaleY), 0.25), 1.5);
        this.panX = (rect.width - contentWidth * this.zoom) / 2 - minX * this.zoom;
        this.panY = (rect.height - contentHeight * this.zoom) / 2 - minY * this.zoom;

        this.updateTransform();
    }

    // Data export/import
    getData() {
        return {
            nodes: this.nodes.map(n => ({
                id: n.id,
                type: n.type,
                name: n.name,
                x: n.x,
                y: n.y,
                config: n.config
            })),
            edges: this.edges.map(e => ({
                from: e.from,
                to: e.to
            }))
        };
    }

    setData(data) {
        this.nodes = data.nodes || [];
        this.edges = data.edges || [];
        this.render();
        this.saveState();
    }

    clear() {
        this.nodes = [];
        this.edges = [];
        this.render();
        this.deselectAll();
        this.saveState();
        this.onChange(this.getData());
    }

    // Drop handler for external drag
    handleDrop(nodeType, x, y) {
        const rect = this.svg.getBoundingClientRect();
        const canvasX = (x - rect.left - this.panX) / this.zoom;
        const canvasY = (y - rect.top - this.panY) / this.zoom;

        return this.addNode(nodeType, canvasX, canvasY);
    }

    // ===================
    // Workflow Validation
    // ===================

    /**
     * Validate the workflow and return validation results
     * @returns {Object} Validation result with isValid boolean and errors array
     */
    validate() {
        const errors = [];
        const warnings = [];

        // Check for start node
        const startNodes = this.nodes.filter(n => n.type === 'start');
        if (startNodes.length === 0) {
            errors.push({ type: 'error', message: 'Workflow must have a Start node' });
        } else if (startNodes.length > 1) {
            errors.push({ type: 'error', message: 'Workflow can only have one Start node' });
        }

        // Check for end node
        const endNodes = this.nodes.filter(n => n.type === 'end');
        if (endNodes.length === 0) {
            errors.push({ type: 'error', message: 'Workflow must have an End node' });
        }

        // Check for orphan nodes (no incoming or outgoing edges, except start/end)
        this.nodes.forEach(node => {
            if (node.type === 'start') {
                const outgoing = this.edges.filter(e => e.from === node.id);
                if (outgoing.length === 0) {
                    errors.push({ type: 'error', message: `Start node must have at least one outgoing connection`, nodeId: node.id });
                }
            } else if (node.type === 'end') {
                const incoming = this.edges.filter(e => e.to === node.id);
                if (incoming.length === 0) {
                    errors.push({ type: 'error', message: `End node must have at least one incoming connection`, nodeId: node.id });
                }
            } else {
                const incoming = this.edges.filter(e => e.to === node.id);
                const outgoing = this.edges.filter(e => e.from === node.id);

                if (incoming.length === 0 && outgoing.length === 0) {
                    errors.push({ type: 'error', message: `Node "${node.name}" is not connected to anything`, nodeId: node.id });
                } else if (incoming.length === 0) {
                    warnings.push({ type: 'warning', message: `Node "${node.name}" has no incoming connections`, nodeId: node.id });
                } else if (outgoing.length === 0 && node.type !== 'end') {
                    warnings.push({ type: 'warning', message: `Node "${node.name}" has no outgoing connections`, nodeId: node.id });
                }
            }
        });

        // Check for cycles (using DFS)
        const visited = new Set();
        const recursionStack = new Set();

        const hasCycle = (nodeId, path = []) => {
            if (recursionStack.has(nodeId)) {
                return true;
            }
            if (visited.has(nodeId)) {
                return false;
            }

            visited.add(nodeId);
            recursionStack.add(nodeId);

            const outgoing = this.edges.filter(e => e.from === nodeId);
            for (const edge of outgoing) {
                if (hasCycle(edge.to, [...path, nodeId])) {
                    return true;
                }
            }

            recursionStack.delete(nodeId);
            return false;
        };

        // Check from each start node
        for (const startNode of startNodes) {
            visited.clear();
            recursionStack.clear();
            if (hasCycle(startNode.id)) {
                errors.push({ type: 'error', message: 'Workflow contains a cycle, which is not allowed' });
                break;
            }
        }

        // Check reachability from start to end
        if (startNodes.length === 1 && endNodes.length > 0) {
            const reachable = new Set();
            const queue = [startNodes[0].id];

            while (queue.length > 0) {
                const current = queue.shift();
                if (reachable.has(current)) continue;
                reachable.add(current);

                const outgoing = this.edges.filter(e => e.from === current);
                for (const edge of outgoing) {
                    queue.push(edge.to);
                }
            }

            const unreachableEnd = endNodes.some(end => !reachable.has(end.id));
            if (unreachableEnd) {
                errors.push({ type: 'error', message: 'Not all End nodes are reachable from Start' });
            }

            // Check for unreachable nodes
            const unreachableNodes = this.nodes.filter(n => !reachable.has(n.id) && n.type !== 'start');
            unreachableNodes.forEach(node => {
                warnings.push({ type: 'warning', message: `Node "${node.name}" is not reachable from Start`, nodeId: node.id });
            });
        }

        // Node-specific validation
        this.nodes.forEach(node => {
            if (node.type === 'llm') {
                if (!node.config?.provider || !node.config?.model) {
                    warnings.push({ type: 'warning', message: `LLM node "${node.name}" is missing provider or model configuration`, nodeId: node.id });
                }
            }
            // Add more node-specific validation here
        });

        const result = {
            isValid: errors.length === 0,
            errors,
            warnings
        };

        this.onValidation(result);
        return result;
    }

    /**
     * Highlight validation errors on the canvas
     * @param {Array} errors - Array of error objects with nodeId
     */
    highlightErrors(errors) {
        // Reset all nodes first
        this.nodes.forEach(node => {
            const group = this.nodesGroup.querySelector(`#node-${node.id}`);
            if (group) {
                const rect = group.querySelector('.node-bg');
                const colors = this.nodeColors[node.type] || { stroke: '#4b5563' };
                rect.setAttribute('stroke', colors.stroke);
                rect.setAttribute('stroke-width', '2');
            }
        });

        // Highlight error nodes
        errors.forEach(error => {
            if (error.nodeId) {
                const group = this.nodesGroup.querySelector(`#node-${error.nodeId}`);
                if (group) {
                    const rect = group.querySelector('.node-bg');
                    rect.setAttribute('stroke', '#ef4444');
                    rect.setAttribute('stroke-width', '3');
                }
            }
        });
    }

    // ========================
    // Execution Status Display
    // ========================

    /**
     * Set execution status for a single node
     * @param {string} nodeId - The node ID
     * @param {string} status - Status: 'pending', 'running', 'completed', 'failed', 'cancelled', 'skipped'
     */
    setNodeStatus(nodeId, status) {
        this.nodeStatuses[nodeId] = status;
        this.renderNodeStatus(nodeId);
    }

    /**
     * Set execution statuses for multiple nodes at once
     * @param {Object} statuses - Object mapping nodeId to status
     */
    setNodeStatuses(statuses) {
        this.nodeStatuses = { ...this.nodeStatuses, ...statuses };
        Object.keys(statuses).forEach(nodeId => {
            this.renderNodeStatus(nodeId);
        });
    }

    /**
     * Clear all execution statuses
     */
    clearNodeStatuses() {
        Object.keys(this.nodeStatuses).forEach(nodeId => {
            this.nodeStatuses[nodeId] = null;
            this.renderNodeStatus(nodeId);
        });
        this.nodeStatuses = {};
    }

    /**
     * Render the execution status badge for a node
     * @param {string} nodeId - The node ID
     */
    renderNodeStatus(nodeId) {
        const node = this.nodes.find(n => n.id === nodeId);
        if (!node) return;

        const group = this.nodesGroup.querySelector(`#node-${nodeId}`);
        if (!group) return;

        // Remove existing status elements
        const existingBadge = group.querySelector('.status-badge');
        if (existingBadge) existingBadge.remove();
        const existingOverlay = group.querySelector('.status-overlay');
        if (existingOverlay) existingOverlay.remove();

        const status = this.nodeStatuses[nodeId];
        if (!status) {
            // Reset node stroke to default
            const rect = group.querySelector('.node-bg');
            const colors = this.nodeColors[node.type] || { stroke: '#4b5563' };
            rect.setAttribute('stroke', colors.stroke);
            rect.setAttribute('stroke-width', '2');
            group.classList.remove('status-running');
            return;
        }

        const statusColor = this.statusColors[status] || this.statusColors.pending;
        const rect = group.querySelector('.node-bg');

        // Add colored stroke to indicate status
        rect.setAttribute('stroke', statusColor.stroke);
        rect.setAttribute('stroke-width', '3');

        // Add pulsing class for running state
        if (status === 'running') {
            group.classList.add('status-running');
        } else {
            group.classList.remove('status-running');
        }

        // Create status badge group
        const badgeGroup = document.createElementNS('http://www.w3.org/2000/svg', 'g');
        badgeGroup.setAttribute('class', 'status-badge');

        const badgeX = node.x + this.nodeWidth - 16;
        const badgeY = node.y - 8;

        // Badge background circle
        const badgeCircle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
        badgeCircle.setAttribute('cx', badgeX);
        badgeCircle.setAttribute('cy', badgeY);
        badgeCircle.setAttribute('r', '10');
        badgeCircle.setAttribute('fill', statusColor.badge);
        badgeCircle.setAttribute('stroke', '#ffffff');
        badgeCircle.setAttribute('stroke-width', '2');
        badgeGroup.appendChild(badgeCircle);

        // Status icon
        const icon = this.createStatusIcon(status, badgeX, badgeY);
        if (icon) {
            badgeGroup.appendChild(icon);
        }

        group.appendChild(badgeGroup);
    }

    /**
     * Create SVG icon for the status badge
     * @param {string} status - The execution status
     * @param {number} cx - Center X position
     * @param {number} cy - Center Y position
     * @returns {SVGElement} The icon element
     */
    createStatusIcon(status, cx, cy) {
        const iconGroup = document.createElementNS('http://www.w3.org/2000/svg', 'g');
        iconGroup.setAttribute('fill', '#ffffff');
        iconGroup.setAttribute('stroke', '#ffffff');
        iconGroup.setAttribute('stroke-width', '1.5');
        iconGroup.setAttribute('stroke-linecap', 'round');
        iconGroup.setAttribute('stroke-linejoin', 'round');

        switch (status) {
            case 'pending':
                // Clock icon
                const clockCircle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
                clockCircle.setAttribute('cx', cx);
                clockCircle.setAttribute('cy', cy);
                clockCircle.setAttribute('r', '4');
                clockCircle.setAttribute('fill', 'none');
                iconGroup.appendChild(clockCircle);
                const clockHand = document.createElementNS('http://www.w3.org/2000/svg', 'path');
                clockHand.setAttribute('d', `M${cx},${cy - 2} L${cx},${cy} L${cx + 2},${cy}`);
                clockHand.setAttribute('fill', 'none');
                iconGroup.appendChild(clockHand);
                break;

            case 'running':
                // Spinner icon (animated)
                const spinnerGroup = document.createElementNS('http://www.w3.org/2000/svg', 'g');
                spinnerGroup.setAttribute('class', 'status-running-spinner');
                spinnerGroup.setAttribute('style', `transform-origin: ${cx}px ${cy}px`);
                const spinnerArc = document.createElementNS('http://www.w3.org/2000/svg', 'path');
                spinnerArc.setAttribute('d', `M${cx},${cy - 5} A5,5 0 1,1 ${cx - 5},${cy}`);
                spinnerArc.setAttribute('fill', 'none');
                spinnerArc.setAttribute('stroke-width', '2');
                spinnerGroup.appendChild(spinnerArc);
                iconGroup.appendChild(spinnerGroup);
                break;

            case 'completed':
                // Checkmark icon
                const checkmark = document.createElementNS('http://www.w3.org/2000/svg', 'path');
                checkmark.setAttribute('d', `M${cx - 4},${cy} L${cx - 1},${cy + 3} L${cx + 4},${cy - 3}`);
                checkmark.setAttribute('fill', 'none');
                checkmark.setAttribute('stroke-width', '2');
                iconGroup.appendChild(checkmark);
                break;

            case 'failed':
                // X icon
                const x1 = document.createElementNS('http://www.w3.org/2000/svg', 'path');
                x1.setAttribute('d', `M${cx - 3},${cy - 3} L${cx + 3},${cy + 3}`);
                x1.setAttribute('fill', 'none');
                x1.setAttribute('stroke-width', '2');
                iconGroup.appendChild(x1);
                const x2 = document.createElementNS('http://www.w3.org/2000/svg', 'path');
                x2.setAttribute('d', `M${cx + 3},${cy - 3} L${cx - 3},${cy + 3}`);
                x2.setAttribute('fill', 'none');
                x2.setAttribute('stroke-width', '2');
                iconGroup.appendChild(x2);
                break;

            case 'cancelled':
                // Stop icon (square)
                const stopRect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
                stopRect.setAttribute('x', cx - 3);
                stopRect.setAttribute('y', cy - 3);
                stopRect.setAttribute('width', '6');
                stopRect.setAttribute('height', '6');
                stopRect.setAttribute('fill', '#ffffff');
                stopRect.setAttribute('stroke', 'none');
                iconGroup.appendChild(stopRect);
                break;

            case 'skipped':
                // Skip icon (double arrow)
                const skip = document.createElementNS('http://www.w3.org/2000/svg', 'path');
                skip.setAttribute('d', `M${cx - 3},${cy - 3} L${cx},${cy} L${cx - 3},${cy + 3} M${cx + 1},${cy - 3} L${cx + 4},${cy} L${cx + 1},${cy + 3}`);
                skip.setAttribute('fill', 'none');
                skip.setAttribute('stroke-width', '1.5');
                iconGroup.appendChild(skip);
                break;

            default:
                return null;
        }

        return iconGroup;
    }

    /**
     * Get keyboard shortcuts for display
     * @returns {Array} Array of shortcut descriptions
     */
    static getKeyboardShortcuts() {
        const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
        const cmd = isMac ? 'Cmd' : 'Ctrl';

        return [
            { key: 'Delete / Backspace', action: 'Delete selected node/edge' },
            { key: `${cmd} + Z`, action: 'Undo' },
            { key: `${cmd} + Shift + Z`, action: 'Redo' },
            { key: `${cmd} + C`, action: 'Copy node' },
            { key: `${cmd} + V`, action: 'Paste node' },
            { key: `${cmd} + D`, action: 'Duplicate node' },
            { key: `${cmd} + A`, action: 'Select all nodes' },
            { key: 'Escape', action: 'Deselect / Cancel' },
            { key: 'Arrow keys', action: 'Move selected node' },
            { key: '+ / -', action: 'Zoom in/out' },
            { key: `${cmd} + 0`, action: 'Reset zoom' },
            { key: 'F', action: 'Fit to content' },
            { key: 'Shift + Drag', action: 'Pan canvas' },
            { key: 'Scroll wheel', action: 'Zoom' },
        ];
    }
}

// Export for use
window.DAGEditor = DAGEditor;
