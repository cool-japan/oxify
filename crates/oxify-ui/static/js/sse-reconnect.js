/**
 * SSE Reconnection Handler
 *
 * Provides automatic reconnection logic for Server-Sent Events (SSE) connections.
 * Features:
 * - Exponential backoff for reconnection attempts
 * - Maximum retry limit
 * - Connection state management
 * - Event callbacks for connection lifecycle
 */

class SSEReconnectHandler {
    constructor(url, options = {}) {
        this.url = url;
        this.options = {
            maxRetries: options.maxRetries || 10,
            initialRetryDelay: options.initialRetryDelay || 1000,
            maxRetryDelay: options.maxRetryDelay || 30000,
            onMessage: options.onMessage || (() => {}),
            onOpen: options.onOpen || (() => {}),
            onError: options.onError || (() => {}),
            onClose: options.onClose || (() => {}),
            reconnectOnError: options.reconnectOnError !== false,
        };

        this.eventSource = null;
        this.retryCount = 0;
        this.retryDelay = this.options.initialRetryDelay;
        this.reconnectTimer = null;
        this.isManuallyClosing = false;
    }

    /**
     * Connect to the SSE endpoint
     */
    connect() {
        if (this.eventSource && this.eventSource.readyState !== EventSource.CLOSED) {
            console.warn('SSE: Already connected or connecting');
            return;
        }

        try {
            console.log(`SSE: Connecting to ${this.url}`);
            this.eventSource = new EventSource(this.url);

            this.eventSource.onopen = (event) => {
                console.log('SSE: Connection opened');
                this.retryCount = 0;
                this.retryDelay = this.options.initialRetryDelay;
                this.options.onOpen(event);
            };

            this.eventSource.onmessage = (event) => {
                this.options.onMessage(event);
            };

            this.eventSource.onerror = (error) => {
                console.error('SSE: Connection error', error);
                this.options.onError(error);

                if (this.eventSource.readyState === EventSource.CLOSED) {
                    console.log('SSE: Connection closed');
                    this.options.onClose(error);

                    if (this.options.reconnectOnError && !this.isManuallyClosing) {
                        this.scheduleReconnect();
                    }
                }
            };

        } catch (error) {
            console.error('SSE: Failed to create EventSource', error);
            this.options.onError(error);

            if (this.options.reconnectOnError && !this.isManuallyClosing) {
                this.scheduleReconnect();
            }
        }
    }

    /**
     * Schedule a reconnection attempt with exponential backoff
     */
    scheduleReconnect() {
        if (this.isManuallyClosing) {
            console.log('SSE: Manual close, not reconnecting');
            return;
        }

        if (this.retryCount >= this.options.maxRetries) {
            console.error(`SSE: Max retries (${this.options.maxRetries}) reached, giving up`);
            this.options.onError(new Error('Max retry attempts reached'));
            return;
        }

        this.retryCount++;

        console.log(`SSE: Reconnecting in ${this.retryDelay}ms (attempt ${this.retryCount}/${this.options.maxRetries})`);

        this.reconnectTimer = setTimeout(() => {
            this.connect();
        }, this.retryDelay);

        // Exponential backoff with jitter
        this.retryDelay = Math.min(
            this.retryDelay * 2 + Math.random() * 1000,
            this.options.maxRetryDelay
        );
    }

    /**
     * Manually close the connection
     */
    close() {
        console.log('SSE: Manually closing connection');
        this.isManuallyClosing = true;

        if (this.reconnectTimer) {
            clearTimeout(this.reconnectTimer);
            this.reconnectTimer = null;
        }

        if (this.eventSource) {
            this.eventSource.close();
            this.eventSource = null;
        }
    }

    /**
     * Check if connection is open
     */
    isConnected() {
        return this.eventSource && this.eventSource.readyState === EventSource.OPEN;
    }

    /**
     * Get current connection state
     */
    getState() {
        if (!this.eventSource) return 'disconnected';

        switch (this.eventSource.readyState) {
            case EventSource.CONNECTING:
                return 'connecting';
            case EventSource.OPEN:
                return 'connected';
            case EventSource.CLOSED:
                return 'disconnected';
            default:
                return 'unknown';
        }
    }
}

/**
 * Global utility function to create SSE connections with auto-reconnect
 */
function createSSEConnection(url, options) {
    const handler = new SSEReconnectHandler(url, options);
    handler.connect();
    return handler;
}

// Export for module systems
if (typeof module !== 'undefined' && module.exports) {
    module.exports = { SSEReconnectHandler, createSSEConnection };
}
