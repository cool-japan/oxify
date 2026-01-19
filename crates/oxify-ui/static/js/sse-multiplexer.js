/**
 * SSE Multiplexer - Monitor multiple execution streams with a single connection
 *
 * Usage:
 *   const multiplexer = new SSEMultiplexer();
 *   multiplexer.subscribe('execution-id-1', (data) => {
 *     console.log('Update for execution-id-1:', data);
 *   });
 *   multiplexer.subscribe('execution-id-2', (data) => {
 *     console.log('Update for execution-id-2:', data);
 *   });
 */

class SSEMultiplexer {
  constructor() {
    this.subscriptions = new Map();
    this.eventSource = null;
    this.reconnectAttempts = 0;
    this.maxReconnectAttempts = 10;
    this.reconnectDelay = 1000; // Start with 1 second
    this.isConnected = false;
  }

  /**
   * Subscribe to updates for a specific execution ID
   * @param {string} executionId - The execution ID to monitor
   * @param {Function} callback - Callback function to receive updates
   * @returns {Function} Unsubscribe function
   */
  subscribe(executionId, callback) {
    if (!this.subscriptions.has(executionId)) {
      this.subscriptions.set(executionId, new Set());
    }
    this.subscriptions.get(executionId).add(callback);

    // Reconnect with updated subscription list
    this.connect();

    // Return unsubscribe function
    return () => {
      this.unsubscribe(executionId, callback);
    };
  }

  /**
   * Unsubscribe from updates for a specific execution ID
   * @param {string} executionId - The execution ID to stop monitoring
   * @param {Function} callback - The callback to remove
   */
  unsubscribe(executionId, callback) {
    const callbacks = this.subscriptions.get(executionId);
    if (callbacks) {
      callbacks.delete(callback);
      if (callbacks.size === 0) {
        this.subscriptions.delete(executionId);
      }
    }

    // Reconnect with updated subscription list
    if (this.subscriptions.size === 0) {
      this.disconnect();
    } else {
      this.connect();
    }
  }

  /**
   * Connect to the multiplexed SSE endpoint
   */
  connect() {
    // Close existing connection if any
    if (this.eventSource) {
      this.eventSource.close();
    }

    // Don't connect if no subscriptions
    if (this.subscriptions.size === 0) {
      return;
    }

    // Build URL with execution IDs
    const ids = Array.from(this.subscriptions.keys()).join(',');
    const url = `/sse/executions?ids=${encodeURIComponent(ids)}`;

    console.log('[SSEMultiplexer] Connecting to:', url);

    this.eventSource = new EventSource(url);

    this.eventSource.addEventListener('open', () => {
      console.log('[SSEMultiplexer] Connected');
      this.isConnected = true;
      this.reconnectAttempts = 0;
      this.reconnectDelay = 1000;
    });

    this.eventSource.addEventListener('execution_update', (event) => {
      try {
        const data = JSON.parse(event.data);
        const executionId = data.execution_id;

        // Dispatch to all subscribers for this execution
        const callbacks = this.subscriptions.get(executionId);
        if (callbacks) {
          callbacks.forEach((callback) => {
            try {
              callback(data);
            } catch (error) {
              console.error('[SSEMultiplexer] Error in callback:', error);
            }
          });
        }

        // If HTMX is available and there's HTML, swap it
        if (window.htmx && data.html) {
          const tempDiv = document.createElement('div');
          tempDiv.innerHTML = data.html;
          const element = tempDiv.firstElementChild;
          if (element && element.id) {
            const target = document.getElementById(element.id);
            if (target) {
              target.outerHTML = element.outerHTML;
            }
          }
        }
      } catch (error) {
        console.error('[SSEMultiplexer] Error parsing event:', error);
      }
    });

    this.eventSource.addEventListener('error', (error) => {
      console.error('[SSEMultiplexer] Connection error:', error);
      this.isConnected = false;

      // Close the connection
      if (this.eventSource) {
        this.eventSource.close();
        this.eventSource = null;
      }

      // Attempt to reconnect with exponential backoff
      if (this.reconnectAttempts < this.maxReconnectAttempts) {
        this.reconnectAttempts++;
        const delay = Math.min(this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1), 30000);
        console.log(`[SSEMultiplexer] Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts}/${this.maxReconnectAttempts})`);

        setTimeout(() => {
          if (this.subscriptions.size > 0) {
            this.connect();
          }
        }, delay);
      } else {
        console.error('[SSEMultiplexer] Max reconnection attempts reached');
      }
    });

    this.eventSource.addEventListener('message', (event) => {
      // Handle standard messages (keep-alive, etc.)
      if (event.data === 'ping') {
        console.log('[SSEMultiplexer] Keep-alive ping received');
      }
    });
  }

  /**
   * Disconnect from the SSE endpoint
   */
  disconnect() {
    console.log('[SSEMultiplexer] Disconnecting');
    if (this.eventSource) {
      this.eventSource.close();
      this.eventSource = null;
    }
    this.isConnected = false;
  }

  /**
   * Get connection status
   * @returns {boolean} True if connected
   */
  isConnectionActive() {
    return this.isConnected && this.eventSource !== null;
  }

  /**
   * Get number of active subscriptions
   * @returns {number} Number of execution IDs being monitored
   */
  getSubscriptionCount() {
    return this.subscriptions.size;
  }
}

// Create a global instance for use across the application
if (typeof window !== 'undefined') {
  window.sseMultiplexer = new SSEMultiplexer();
}

// Export for module systems
if (typeof module !== 'undefined' && module.exports) {
  module.exports = SSEMultiplexer;
}
