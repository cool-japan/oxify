/**
 * User Preferences Manager
 *
 * Centralizes user preference management with localStorage persistence.
 * Provides reactive updates across the UI when preferences change.
 */

class UserPreferences {
    constructor() {
        this.storageKey = 'oxify_preferences';
        this.listeners = {};
        this.defaults = {
            // Appearance
            darkMode: false,
            sidebarCollapsed: false,
            compactMode: false,

            // Editor settings
            editor: {
                snapToGrid: true,
                gridSize: 20,
                showMinimap: false,
                autoSave: true,
                autoSaveInterval: 30000, // 30 seconds
            },

            // List view settings
            lists: {
                workflowsPerPage: 10,
                workflowsSortBy: 'updated_at',
                workflowsSortOrder: 'desc',
                executionsPerPage: 10,
                executionsSortBy: 'started_at',
                executionsSortOrder: 'desc',
            },

            // Notifications
            notifications: {
                showToasts: true,
                toastDuration: 5000,
                soundEnabled: false,
                executionComplete: true,
                executionFailed: true,
            },

            // Recent items
            recentWorkflows: [],
            recentExecutions: [],
        };

        this.preferences = this.load();
        this.applyAll();
    }

    /**
     * Load preferences from localStorage
     * @returns {Object} Merged preferences with defaults
     */
    load() {
        try {
            const stored = localStorage.getItem(this.storageKey);
            if (stored) {
                const parsed = JSON.parse(stored);
                return this.deepMerge(this.defaults, parsed);
            }
        } catch (e) {
            console.warn('Failed to load preferences:', e);
        }
        return { ...this.defaults };
    }

    /**
     * Save preferences to localStorage
     */
    save() {
        try {
            localStorage.setItem(this.storageKey, JSON.stringify(this.preferences));
        } catch (e) {
            console.warn('Failed to save preferences:', e);
        }
    }

    /**
     * Deep merge two objects
     * @param {Object} target - Target object
     * @param {Object} source - Source object
     * @returns {Object} Merged object
     */
    deepMerge(target, source) {
        const result = { ...target };
        for (const key in source) {
            if (source[key] && typeof source[key] === 'object' && !Array.isArray(source[key])) {
                result[key] = this.deepMerge(target[key] || {}, source[key]);
            } else {
                result[key] = source[key];
            }
        }
        return result;
    }

    /**
     * Get a preference value by path (e.g., 'editor.snapToGrid')
     * @param {string} path - Dot-notation path to preference
     * @param {*} defaultValue - Default value if not found
     * @returns {*} The preference value
     */
    get(path, defaultValue = undefined) {
        const parts = path.split('.');
        let value = this.preferences;
        for (const part of parts) {
            if (value === undefined || value === null) {
                return defaultValue;
            }
            value = value[part];
        }
        return value !== undefined ? value : defaultValue;
    }

    /**
     * Set a preference value by path
     * @param {string} path - Dot-notation path to preference
     * @param {*} value - Value to set
     */
    set(path, value) {
        const parts = path.split('.');
        let obj = this.preferences;
        for (let i = 0; i < parts.length - 1; i++) {
            if (!obj[parts[i]]) {
                obj[parts[i]] = {};
            }
            obj = obj[parts[i]];
        }
        const oldValue = obj[parts[parts.length - 1]];
        obj[parts[parts.length - 1]] = value;
        this.save();
        this.notify(path, value, oldValue);
        this.applyPreference(path, value);
    }

    /**
     * Toggle a boolean preference
     * @param {string} path - Dot-notation path to preference
     * @returns {boolean} New value
     */
    toggle(path) {
        const current = this.get(path, false);
        this.set(path, !current);
        return !current;
    }

    /**
     * Reset preferences to defaults
     */
    reset() {
        this.preferences = { ...this.defaults };
        this.save();
        this.applyAll();
        this.notify('*', this.preferences, null);
    }

    /**
     * Subscribe to preference changes
     * @param {string} path - Path to watch (or '*' for all changes)
     * @param {Function} callback - Callback function(newValue, oldValue)
     * @returns {Function} Unsubscribe function
     */
    subscribe(path, callback) {
        if (!this.listeners[path]) {
            this.listeners[path] = [];
        }
        this.listeners[path].push(callback);
        return () => {
            this.listeners[path] = this.listeners[path].filter(cb => cb !== callback);
        };
    }

    /**
     * Notify listeners of a preference change
     * @param {string} path - Changed path
     * @param {*} newValue - New value
     * @param {*} oldValue - Old value
     */
    notify(path, newValue, oldValue) {
        // Notify specific path listeners
        if (this.listeners[path]) {
            this.listeners[path].forEach(cb => cb(newValue, oldValue));
        }
        // Notify wildcard listeners
        if (this.listeners['*']) {
            this.listeners['*'].forEach(cb => cb({ path, newValue, oldValue }));
        }
    }

    /**
     * Apply a specific preference to the UI
     * @param {string} path - Preference path
     * @param {*} value - Preference value
     */
    applyPreference(path, value) {
        switch (path) {
            case 'darkMode':
                document.documentElement.classList.toggle('dark', value);
                break;
            case 'sidebarCollapsed':
                const sidebar = document.getElementById('sidebar');
                if (sidebar) {
                    sidebar.classList.toggle('collapsed', value);
                }
                break;
            case 'compactMode':
                document.body.classList.toggle('compact-mode', value);
                break;
        }
    }

    /**
     * Apply all preferences to the UI
     */
    applyAll() {
        // Apply dark mode
        document.documentElement.classList.toggle('dark', this.get('darkMode', false));
        // Apply compact mode
        document.body.classList.toggle('compact-mode', this.get('compactMode', false));
    }

    /**
     * Add a recent workflow to the list
     * @param {string} id - Workflow ID
     * @param {string} name - Workflow name
     */
    addRecentWorkflow(id, name) {
        let recent = this.get('recentWorkflows', []);
        // Remove if already exists
        recent = recent.filter(w => w.id !== id);
        // Add to front
        recent.unshift({ id, name, timestamp: Date.now() });
        // Keep only last 10
        recent = recent.slice(0, 10);
        this.set('recentWorkflows', recent);
    }

    /**
     * Add a recent execution to the list
     * @param {string} id - Execution ID
     * @param {string} workflowName - Workflow name
     */
    addRecentExecution(id, workflowName) {
        let recent = this.get('recentExecutions', []);
        // Remove if already exists
        recent = recent.filter(e => e.id !== id);
        // Add to front
        recent.unshift({ id, workflowName, timestamp: Date.now() });
        // Keep only last 10
        recent = recent.slice(0, 10);
        this.set('recentExecutions', recent);
    }

    /**
     * Get recent workflows
     * @returns {Array} Recent workflows
     */
    getRecentWorkflows() {
        return this.get('recentWorkflows', []);
    }

    /**
     * Get recent executions
     * @returns {Array} Recent executions
     */
    getRecentExecutions() {
        return this.get('recentExecutions', []);
    }

    /**
     * Export preferences as JSON
     * @returns {string} JSON string
     */
    export() {
        return JSON.stringify(this.preferences, null, 2);
    }

    /**
     * Import preferences from JSON
     * @param {string} json - JSON string
     * @returns {boolean} Success
     */
    import(json) {
        try {
            const parsed = JSON.parse(json);
            this.preferences = this.deepMerge(this.defaults, parsed);
            this.save();
            this.applyAll();
            return true;
        } catch (e) {
            console.error('Failed to import preferences:', e);
            return false;
        }
    }
}

// Create global instance
window.userPreferences = new UserPreferences();

// Export for module systems
if (typeof module !== 'undefined' && module.exports) {
    module.exports = UserPreferences;
}
