/**
 * Easy-writing · Character Nebula Force Graph Bridge
 * 高性能 Canvas 2D 星云力导向拓扑引擎 (复刻辐射爆炸图效果)
 */

class CharacterGraphEngine {
  constructor(canvas, options = {}) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d');
    this.options = {
      onNodeClick: options.onNodeClick || null,
      onNodeHover: options.onNodeHover || null,
      theme: options.theme || 'light', // 'light' | 'dark'
      ...options
    };

    this.nodes = [];
    this.links = [];
    this.nodeMap = new Map();
    this.activeNodeId = null;

    // Viewport transform
    this.transform = {
      x: 0,
      y: 0,
      scale: 1.0,
      targetX: 0,
      targetY: 0,
      targetScale: 1.0
    };

    // Interaction states
    this.isDragging = false;
    this.draggedNode = null;
    this.lastMouse = { x: 0, y: 0 };
    this.hoveredNode = null;

    // Animation & simulation
    this.animFrameId = null;
    this.pulseAngle = 0;
    this.alpha = 1.0; // Force simulation cooling

    // Bind events
    this.initEvents();
    this.resize();
    this.startLoop();
  }

  resize() {
    const rect = this.canvas.parentElement.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    this.width = rect.width;
    this.height = rect.height;
    this.canvas.width = rect.width * dpr;
    this.canvas.height = rect.height * dpr;
    this.canvas.style.width = `${rect.width}px`;
    this.canvas.style.height = `${rect.height}px`;
    this.ctx.scale(dpr, dpr);
  }

  setData(nodesData, linksData, activeId = null) {
    this.activeNodeId = activeId || (nodesData.length > 0 ? nodesData[0].id : null);

    // Color palette for factions
    const factionColors = {
      '默认阵营': { primary: '#f97316', secondary: '#ffedd5', border: '#ea580c' },
      '主角团': { primary: '#f97316', secondary: '#ffedd5', border: '#ea580c' },
      '同盟': { primary: '#10b981', secondary: '#d1fae5', border: '#059669' },
      '宗门': { primary: '#059669', secondary: '#ecfdf5', border: '#047857' },
      '宿敌': { primary: '#0284c7', secondary: '#e0f2fe', border: '#0369a1' },
      '对立': { primary: '#2563eb', secondary: '#dbeafe', border: '#1d4ed8' },
      '中立': { primary: '#8b5cf6', secondary: '#ede9fe', border: '#7c3aed' },
      '羁绊': { primary: '#f43f5e', secondary: '#ffe4e6', border: '#e11d48' },
    };

    // Preserve existing node positions if re-rendering
    const oldPositions = new Map();
    this.nodes.forEach(n => oldPositions.set(n.id, { x: n.x, y: n.y, vx: n.vx, vy: n.vy }));

    this.nodes = nodesData.map((d, index) => {
      const old = oldPositions.get(d.id);
      const angle = (index / Math.max(1, nodesData.length)) * Math.PI * 2;
      const r = 120 + Math.random() * 80;
      
      const faction = d.faction || '默认阵营';
      const colors = factionColors[faction] || factionColors['默认阵营'];
      const importance = d.importance || 2;
      const radius = importance === 1 ? 24 : (importance === 2 ? 18 : 12);

      return {
        id: d.id,
        name: d.name || '无名',
        faction: faction,
        colors: colors,
        radius: radius,
        importance: importance,
        summary: d.summary || '',
        x: old ? old.x : (this.width / 2 + Math.cos(angle) * r),
        y: old ? old.y : (this.height / 2 + Math.sin(angle) * r),
        vx: old ? old.vx : (Math.random() - 0.5) * 2,
        vy: old ? old.vy : (Math.random() - 0.5) * 2,
      };
    });

    this.nodeMap = new Map();
    this.nodes.forEach(n => this.nodeMap.set(n.id, n));

    this.links = linksData.map(l => ({
      id: l.id,
      sourceId: l.source,
      targetId: l.target,
      relationName: l.relation_name || '',
      description: l.description || '',
    }));

    this.alpha = 1.0; // restart simulation
    if (this.activeNodeId) {
      this.centerOnNode(this.activeNodeId, true);
    }
  }

  setActiveNode(nodeId) {
    this.activeNodeId = nodeId;
    this.centerOnNode(nodeId, false);
    this.alpha = 0.8; // boost simulation slightly
  }

  centerOnNode(nodeId, immediate = false) {
    const node = this.nodeMap.get(nodeId);
    if (!node) return;

    this.transform.targetX = this.width / 2 - node.x * this.transform.scale;
    this.transform.targetY = this.height / 2 - node.y * this.transform.scale;

    if (immediate) {
      this.transform.x = this.transform.targetX;
      this.transform.y = this.transform.targetY;
    }
  }

  // Force simulation tick
  simulate() {
    if (this.alpha < 0.005) return;
    this.alpha *= 0.985; // cooling parameter

    const cx = this.width / 2;
    const cy = this.height / 2;
    const activeNode = this.nodeMap.get(this.activeNodeId);

    // 1. Center gravity & radial layout constraints
    for (const node of this.nodes) {
      if (node === this.draggedNode) continue;

      if (node.id === this.activeNodeId) {
        // Active node strongly attracted to current center
        node.vx += (cx - node.x) * 0.08;
        node.vy += (cy - node.y) * 0.08;
      } else {
        // Slight global center gravity
        node.vx += (cx - node.x) * 0.003;
        node.vy += (cy - node.y) * 0.003;
      }
    }

    // 2. Node-Node Repulsion (Coulomb law)
    for (let i = 0; i < this.nodes.length; i++) {
      const n1 = this.nodes[i];
      for (let j = i + 1; j < this.nodes.length; j++) {
        const n2 = this.nodes[j];
        const dx = n2.x - n1.x;
        const dy = n2.y - n1.y;
        const dist = Math.sqrt(dx * dx + dy * dy) || 1;
        const minDist = n1.radius + n2.radius + 40;

        if (dist < 400) {
          const force = (5000 / (dist * dist)) * this.alpha;
          const fx = (dx / dist) * force;
          const fy = (dy / dist) * force;
          if (n1 !== this.draggedNode) { n1.vx -= fx; n1.vy -= fy; }
          if (n2 !== this.draggedNode) { n2.vx += fx; n2.vy += fy; }
        }
      }
    }

    // 3. Link spring attraction (Hooke's law)
    for (const link of this.links) {
      const n1 = this.nodeMap.get(link.sourceId);
      const n2 = this.nodeMap.get(link.targetId);
      if (!n1 || !n2) continue;

      const isDirect = (n1.id === this.activeNodeId || n2.id === this.activeNodeId);
      const targetDist = isDirect ? 140 : 100; // Direct links form clear primary orbit
      const stiffness = isDirect ? 0.05 : 0.02;

      const dx = n2.x - n1.x;
      const dy = n2.y - n1.y;
      const dist = Math.sqrt(dx * dx + dy * dy) || 1;
      const force = (dist - targetDist) * stiffness * this.alpha;
      const fx = (dx / dist) * force;
      const fy = (dy / dist) * force;

      if (n1 !== this.draggedNode) { n1.vx += fx; n1.vy += fy; }
      if (n2 !== this.draggedNode) { n2.vx -= fx; n2.vy -= fy; }
    }

    // 4. Update positions with damping
    for (const node of this.nodes) {
      if (node === this.draggedNode) continue;
      node.vx *= 0.65;
      node.vy *= 0.65;
      node.x += node.vx;
      node.y += node.vy;
    }
  }

  // Smooth camera interpolation
  updateCamera() {
    this.transform.x += (this.transform.targetX - this.transform.x) * 0.12;
    this.transform.y += (this.transform.targetY - this.transform.y) * 0.12;
    this.transform.scale += (this.transform.targetScale - this.transform.scale) * 0.12;
  }

  // Main render loop
  render() {
    this.simulate();
    this.updateCamera();
    this.pulseAngle += 0.04;

    const ctx = this.ctx;
    ctx.clearRect(0, 0, this.width, this.height);

    ctx.save();
    // Apply viewport transform
    ctx.translate(this.transform.x, this.transform.y);
    ctx.scale(this.transform.scale, this.transform.scale);

    const activeNode = this.nodeMap.get(this.activeNodeId);

    // Identify 1-hop direct neighbors
    const directNeighborIds = new Set();
    if (activeNode) {
      directNeighborIds.add(activeNode.id);
      for (const l of this.links) {
        if (l.sourceId === activeNode.id) directNeighborIds.add(l.targetId);
        if (l.targetId === activeNode.id) directNeighborIds.add(l.sourceId);
      }
    }

    // ==========================================
    // LAYER 1: Deep Background Ghost Network
    // ==========================================
    ctx.lineWidth = 1;
    for (const link of this.links) {
      const isDirect = (link.sourceId === this.activeNodeId || link.targetId === this.activeNodeId);
      if (isDirect) continue; // Drawn in Layer 2

      const n1 = this.nodeMap.get(link.sourceId);
      const n2 = this.nodeMap.get(link.targetId);
      if (!n1 || !n2) continue;

      ctx.beginPath();
      ctx.moveTo(n1.x, n1.y);
      ctx.lineTo(n2.x, n2.y);
      ctx.strokeStyle = 'rgba(148, 163, 184, 0.22)';
      ctx.stroke();
    }

    // ==========================================
    // LAYER 2: Active Radiation Rays & Highlights
    // ==========================================
    for (const link of this.links) {
      const isDirect = (link.sourceId === this.activeNodeId || link.targetId === this.activeNodeId);
      if (!isDirect) continue;

      const n1 = this.nodeMap.get(link.sourceId);
      const n2 = this.nodeMap.get(link.targetId);
      if (!n1 || !n2) continue;

      // Glowing direct line
      ctx.save();
      ctx.beginPath();
      ctx.moveTo(n1.x, n1.y);
      ctx.lineTo(n2.x, n2.y);
      ctx.lineWidth = 2.2;
      ctx.strokeStyle = '#3b82f6';
      ctx.shadowColor = '#60a5fa';
      ctx.shadowBlur = 8;
      ctx.stroke();
      ctx.restore();

      // Relation Label Tag Badge
      if (link.relationName && this.transform.scale > 0.45) {
        const midX = (n1.x + n2.x) / 2;
        const midY = (n1.y + n2.y) / 2;
        ctx.save();
        ctx.font = '10px -apple-system, sans-serif';
        const textWidth = ctx.measureText(link.relationName).width;
        
        ctx.fillStyle = 'rgba(255, 255, 255, 0.92)';
        ctx.strokeStyle = '#cbd5e1';
        ctx.lineWidth = 0.8;
        ctx.beginPath();
        ctx.roundRect(midX - textWidth / 2 - 5, midY - 7, textWidth + 10, 14, 3);
        ctx.fill();
        ctx.stroke();

        ctx.fillStyle = '#1e3a8a';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText(link.relationName, midX, midY);
        ctx.restore();
      }
    }

    // ==========================================
    // LAYER 3: Nodes (Ghost vs Exploded Primary)
    // ==========================================
    // 3.1 Draw ghost (unrelated) nodes first
    for (const node of this.nodes) {
      if (directNeighborIds.has(node.id)) continue;

      ctx.save();
      ctx.globalAlpha = 0.28;
      ctx.beginPath();
      ctx.arc(node.x, node.y, node.radius * 0.75, 0, Math.PI * 2);
      ctx.fillStyle = node.colors.primary;
      ctx.fill();

      if (this.transform.scale > 0.75) {
        ctx.font = '9px sans-serif';
        ctx.fillStyle = '#64748b';
        ctx.textAlign = 'center';
        ctx.fillText(node.name, node.x, node.y + node.radius + 9);
      }
      ctx.restore();
    }

    // 3.2 Draw 1-hop primary neighbor nodes
    for (const node of this.nodes) {
      if (!directNeighborIds.has(node.id) || node.id === this.activeNodeId) continue;

      ctx.save();
      // Outer subtle glow
      ctx.beginPath();
      ctx.arc(node.x, node.y, node.radius + 4, 0, Math.PI * 2);
      ctx.fillStyle = node.colors.secondary;
      ctx.fill();

      // Core Circle
      ctx.beginPath();
      ctx.arc(node.x, node.y, node.radius, 0, Math.PI * 2);
      ctx.fillStyle = node.colors.primary;
      ctx.strokeStyle = '#ffffff';
      ctx.lineWidth = 2;
      ctx.fill();
      ctx.stroke();

      // Display name
      ctx.fillStyle = '#ffffff';
      ctx.font = 'bold 11px sans-serif';
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      const shortName = node.name.slice(0, 2);
      ctx.fillText(shortName, node.x, node.y);

      // Full name under node
      ctx.fillStyle = '#1e293b';
      ctx.font = '10px -apple-system, sans-serif';
      ctx.fillText(node.name, node.x, node.y + node.radius + 12);

      ctx.restore();
    }

    // 3.3 Draw Sun-Core Active Center Node
    if (activeNode) {
      ctx.save();
      const cx = activeNode.x;
      const cy = activeNode.y;
      const r = activeNode.radius + 4;

      // Sun Spikes (放射芒刺) - 参考图片标志性特征
      const spikeCount = 28;
      ctx.strokeStyle = 'rgba(249, 115, 22, 0.45)';
      ctx.lineWidth = 1.5;
      for (let i = 0; i < spikeCount; i++) {
        const angle = (i / spikeCount) * Math.PI * 2 + this.pulseAngle * 0.2;
        const innerR = r + 6;
        const outerR = r + 14 + (i % 2 === 0 ? 6 : 0);
        ctx.beginPath();
        ctx.moveTo(cx + Math.cos(angle) * innerR, cy + Math.sin(angle) * innerR);
        ctx.lineTo(cx + Math.cos(angle) * outerR, cy + Math.sin(angle) * outerR);
        ctx.stroke();
      }

      // Outer Pulsing Glow
      const pulseSize = Math.sin(this.pulseAngle) * 4;
      ctx.beginPath();
      ctx.arc(cx, cy, r + 10 + pulseSize, 0, Math.PI * 2);
      ctx.fillStyle = 'rgba(251, 146, 60, 0.2)';
      ctx.fill();

      // Center Node Core
      ctx.beginPath();
      ctx.arc(cx, cy, r, 0, Math.PI * 2);
      ctx.fillStyle = '#ea580c';
      ctx.shadowColor = '#f97316';
      ctx.shadowBlur = 15;
      ctx.strokeStyle = '#ffffff';
      ctx.lineWidth = 3;
      ctx.fill();
      ctx.stroke();

      // Center Name
      ctx.fillStyle = '#ffffff';
      ctx.font = 'bold 12px sans-serif';
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(activeNode.name.slice(0, 3), cx, cy);

      // Label with badge
      ctx.fillStyle = '#c2410c';
      ctx.font = 'bold 11px sans-serif';
      ctx.fillText(`★ ${activeNode.name}`, cx, cy + r + 16);

      ctx.restore();
    }

    ctx.restore();
  }

  startLoop() {
    const loop = () => {
      this.render();
      this.animFrameId = requestAnimationFrame(loop);
    };
    this.animFrameId = requestAnimationFrame(loop);
  }

  // Screen to Canvas World Coordinates
  screenToWorld(sx, sy) {
    return {
      x: (sx - this.transform.x) / this.transform.scale,
      y: (sy - this.transform.y) / this.transform.scale
    };
  }

  initEvents() {
    const getPos = (e) => {
      const rect = this.canvas.getBoundingClientRect();
      return { x: e.clientX - rect.left, y: e.clientY - rect.top };
    };

    this.canvas.addEventListener('mousedown', (e) => {
      const pos = getPos(e);
      const world = this.screenToWorld(pos.x, pos.y);

      // Find clicked node
      let clickedNode = null;
      for (const node of this.nodes) {
        const dx = node.x - world.x;
        const dy = node.y - world.y;
        if (Math.sqrt(dx * dx + dy * dy) <= node.radius + 6) {
          clickedNode = node;
          break;
        }
      }

      if (clickedNode) {
        this.draggedNode = clickedNode;
        this.alpha = 0.5; // wake up physics
      } else {
        this.isDragging = true;
        this.lastMouse = pos;
      }
    });

    window.addEventListener('mousemove', (e) => {
      const pos = getPos(e);
      if (this.draggedNode) {
        const world = this.screenToWorld(pos.x, pos.y);
        this.draggedNode.x = world.x;
        this.draggedNode.y = world.y;
        this.draggedNode.vx = 0;
        this.draggedNode.vy = 0;
      } else if (this.isDragging) {
        const dx = pos.x - this.lastMouse.x;
        const dy = pos.y - this.lastMouse.y;
        this.transform.targetX += dx;
        this.transform.targetY += dy;
        this.transform.x += dx;
        this.transform.y += dy;
        this.lastMouse = pos;
      }
    });

    window.addEventListener('mouseup', (e) => {
      if (this.draggedNode) {
        // If clicked without large drag, trigger node click
        const node = this.draggedNode;
        this.draggedNode = null;
        if (this.options.onNodeClick) {
          this.options.onNodeClick(node.id, node.name);
        }
      }
      this.isDragging = false;
    });

    // Zoom on wheel
    this.canvas.addEventListener('wheel', (e) => {
      e.preventDefault();
      const pos = getPos(e);
      const zoomFactor = e.deltaY < 0 ? 1.15 : 0.87;
      const newScale = Math.max(0.2, Math.min(3.5, this.transform.scale * zoomFactor));

      // Zoom towards mouse pointer
      this.transform.targetX = pos.x - (pos.x - this.transform.x) * (newScale / this.transform.scale);
      this.transform.targetY = pos.y - (pos.y - this.transform.y) * (newScale / this.transform.scale);
      this.transform.targetScale = newScale;
    }, { passive: false });

    // Window resize
    window.addEventListener('resize', () => this.resize());
  }

  zoomIn() {
    this.transform.targetScale = Math.min(3.5, this.transform.scale * 1.3);
  }

  zoomOut() {
    this.transform.targetScale = Math.max(0.2, this.transform.scale * 0.7);
  }

  resetView() {
    this.transform.targetScale = 1.0;
    if (this.activeNodeId) {
      this.centerOnNode(this.activeNodeId, false);
    } else {
      this.transform.targetX = 0;
      this.transform.targetY = 0;
    }
  }

  destroy() {
    if (this.animFrameId) {
      cancelAnimationFrame(this.animFrameId);
    }
  }
}

// Global Export for Bridge
window.CharacterGraphBridge = {
  instances: new Map(),

  mount: function(canvasId, optionsJson) {
    const canvas = document.getElementById(canvasId);
    if (!canvas) return false;

    let opts = {};
    if (typeof optionsJson === 'string') {
      try { opts = JSON.parse(optionsJson); } catch (e) {}
    } else if (typeof optionsJson === 'object') {
      opts = optionsJson;
    }

    // Callbacks bridged to global custom events
    opts.onNodeClick = (nodeId, nodeName) => {
      const event = new CustomEvent('character-graph-node-click', {
        detail: { nodeId, nodeName }
      });
      window.dispatchEvent(event);
    };

    const engine = new CharacterGraphEngine(canvas, opts);
    this.instances.set(canvasId, engine);
    return true;
  },

  setData: function(canvasId, nodesJson, linksJson, activeId) {
    const engine = this.instances.get(canvasId);
    if (!engine) return false;

    const nodes = typeof nodesJson === 'string' ? JSON.parse(nodesJson) : nodesJson;
    const links = typeof linksJson === 'string' ? JSON.parse(linksJson) : linksJson;
    engine.setData(nodes, links, activeId);
    return true;
  },

  setActiveNode: function(canvasId, nodeId) {
    const engine = this.instances.get(canvasId);
    if (!engine) return false;
    engine.setActiveNode(nodeId);
    return true;
  },

  zoomIn: function(canvasId) {
    const engine = this.instances.get(canvasId);
    if (engine) engine.zoomIn();
  },

  zoomOut: function(canvasId) {
    const engine = this.instances.get(canvasId);
    if (engine) engine.zoomOut();
  },

  resetView: function(canvasId) {
    const engine = this.instances.get(canvasId);
    if (engine) engine.resetView();
  },

  destroy: function(canvasId) {
    const engine = this.instances.get(canvasId);
    if (engine) {
      engine.destroy();
      this.instances.delete(canvasId);
    }
  }
};
