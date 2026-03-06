// Set to true to enable verbose console output during development
const DEBUG = false;
function dbg(...args) { if (DEBUG) console.log(...args); }

class RTCStreamerClient {
    constructor() {
        this.ws = null;
        this.pc = null;
        this.videoElement = document.getElementById('videoPlayer');
        this.statusElement = document.getElementById('status');
        this.connStatusElement = document.getElementById('connStatus');
        this.bitrateElement = document.getElementById('bitrate');
        this.packetsLostElement = document.getElementById('packetsLost');
        this.latencyElement = document.getElementById('latency');
        this.loadingElement = document.getElementById('loading');
        this.connectBtn = document.getElementById('connectBtn');
        this.disconnectBtn = document.getElementById('disconnectBtn');

        // ICE servers received from the server via the 'config' message
        this.iceServers = null;
        // Previous stats snapshot for delta-based bitrate calculation
        this.prevBytesReceived = 0;
        this.prevStatsTime = null;

        this.setupEventListeners();
        this.statsInterval = null;
    }

    setupEventListeners() {
        this.connectBtn.addEventListener('click', () => this.connect());
        this.disconnectBtn.addEventListener('click', () => this.disconnect());
    }

    async connect() {
        try {
            this.updateStatus('Connecting...', 'connecting');
            this.showLoading(true);
            this.connectBtn.disabled = true;

            // Use wss:// when the page is served over HTTPS to avoid mixed-content errors
            const wsProtocol = location.protocol === 'https:' ? 'wss://' : 'ws://';
            const wsUrl = `${wsProtocol}${window.location.host}/signal`;
            this.ws = new WebSocket(wsUrl);

            this.ws.onopen = () => {
                dbg('WebSocket connected');
                this.sendMessage({ type: 'watch' });
            };

            this.ws.onmessage = async (event) => {
                const message = JSON.parse(event.data);
                await this.handleSignalingMessage(message);
            };

            this.ws.onerror = () => {
                this.updateStatus('Connection error', 'error');
                this.showLoading(false);
                this.connectBtn.disabled = false;
            };

            this.ws.onclose = () => {
                dbg('WebSocket closed');
                this.updateStatus('Disconnected', 'disconnected');
                this.showLoading(false);
                this.connectBtn.disabled = false;
                this.disconnectBtn.disabled = true;
                this.stopStats();
            };

        } catch (error) {
            this.updateStatus('Failed to connect', 'error');
            this.showLoading(false);
            this.connectBtn.disabled = false;
        }
    }

    async handleSignalingMessage(message) {
        dbg('Received signaling message:', message.type);

        switch (message.type) {
            case 'config':
                // Store ICE servers sent by the server so handleOffer can use them
                this.iceServers = message.ice_servers.map(url => ({ urls: url }));
                dbg('ICE servers configured:', this.iceServers.length);
                break;
            case 'offer':
                await this.handleOffer(message.sdp);
                break;
            case 'ice-candidate':
                await this.handleIceCandidate(message.candidate);
                break;
            case 'error':
                this.updateStatus(`Error: ${message.message}`, 'error');
                this.showLoading(false);
                this.connectBtn.disabled = false;
                break;
        }
    }

    async handleOffer(sdp) {
        dbg('Handling offer');

        // Use ICE servers from the server config, or fall back to a default
        const iceServers = this.iceServers || [{ urls: 'stun:stun.l.google.com:19302' }];
        this.pc = new RTCPeerConnection({ iceServers });

        // Handle incoming tracks
        this.pc.ontrack = (event) => {
            dbg('Received track:', event.track.kind);
            if (event.track.kind === 'video') {
                this.videoElement.srcObject = event.streams[0];
                this.showLoading(false);
                this.updateStatus('Connected', 'connected');
                this.disconnectBtn.disabled = false;
                this.startStats();
            }
        };

        // Handle ICE candidates
        this.pc.onicecandidate = (event) => {
            if (event.candidate) {
                dbg('Sending ICE candidate');
                this.sendMessage({
                    type: 'ice-candidate',
                    candidate: event.candidate.candidate
                });
            }
        };

        // Handle connection state changes
        this.pc.onconnectionstatechange = () => {
            dbg('Connection state:', this.pc.connectionState);
            this.connStatusElement.textContent = this.pc.connectionState;

            if (this.pc.connectionState === 'failed' ||
                this.pc.connectionState === 'disconnected') {
                this.updateStatus('Connection lost', 'error');
                this.showLoading(false);
            }
        };

        // Set remote description (offer)
        await this.pc.setRemoteDescription(new RTCSessionDescription({
            type: 'offer',
            sdp: sdp
        }));

        // Create answer
        const answer = await this.pc.createAnswer();
        await this.pc.setLocalDescription(answer);

        // Send answer
        this.sendMessage({
            type: 'answer',
            sdp: answer.sdp
        });

        dbg('Answer sent');
    }

    async handleIceCandidate(candidate) {
        if (this.pc) {
            try {
                await this.pc.addIceCandidate(new RTCIceCandidate({ candidate }));
                dbg('ICE candidate added');
            } catch (error) {
                // Ignore — ICE candidate errors are non-fatal
            }
        }
    }

    sendMessage(message) {
        if (this.ws && this.ws.readyState === WebSocket.OPEN) {
            this.ws.send(JSON.stringify(message));
        }
    }

    disconnect() {
        if (this.pc) {
            this.pc.close();
            this.pc = null;
        }

        if (this.ws) {
            this.ws.close();
            this.ws = null;
        }

        this.videoElement.srcObject = null;
        this.updateStatus('Disconnected', 'disconnected');
        this.connectBtn.disabled = false;
        this.disconnectBtn.disabled = true;
        this.stopStats();
    }

    updateStatus(message, className) {
        this.statusElement.textContent = message;
        this.statusElement.className = `status ${className}`;
    }

    showLoading(show) {
        this.loadingElement.style.display = show ? 'flex' : 'none';
    }

    startStats() {
        this.prevBytesReceived = 0;
        this.prevStatsTime = null;

        this.statsInterval = setInterval(async () => {
            if (!this.pc) return;

            try {
                const stats = await this.pc.getStats();
                const now = Date.now();
                let bytesReceived = 0;
                let packetsLost = 0;
                let roundTripTime = null;

                stats.forEach(report => {
                    if (report.type === 'inbound-rtp' && report.kind === 'video') {
                        bytesReceived = report.bytesReceived || 0;
                        packetsLost = report.packetsLost || 0;
                        // roundTripTime may be present on inbound-rtp in some browsers
                        if (report.roundTripTime != null) {
                            roundTripTime = report.roundTripTime;
                        }
                    }
                    // candidate-pair gives us a more reliable RTT source
                    if (report.type === 'candidate-pair' && report.state === 'succeeded' &&
                            report.currentRoundTripTime != null) {
                        roundTripTime = report.currentRoundTripTime;
                    }
                });

                // Delta-based bitrate: compute bytes received since last sample
                let bitrateKbps = 0;
                if (this.prevStatsTime !== null) {
                    const elapsedSec = (now - this.prevStatsTime) / 1000;
                    const bytesDelta = bytesReceived - this.prevBytesReceived;
                    bitrateKbps = elapsedSec > 0
                        ? Math.round((bytesDelta * 8) / (elapsedSec * 1000))
                        : 0;
                }
                this.prevBytesReceived = bytesReceived;
                this.prevStatsTime = now;

                this.bitrateElement.textContent = `${bitrateKbps} kbps`;
                this.packetsLostElement.textContent = packetsLost;

                if (roundTripTime != null) {
                    this.latencyElement.textContent = `${Math.round(roundTripTime * 1000)} ms`;
                }

            } catch (error) {
                // Stats errors are non-fatal; ignore silently
            }
        }, 1000);
    }

    stopStats() {
        if (this.statsInterval) {
            clearInterval(this.statsInterval);
            this.statsInterval = null;
        }
        this.bitrateElement.textContent = '0 kbps';
        this.packetsLostElement.textContent = '0';
        this.latencyElement.textContent = '— ms';
        this.connStatusElement.textContent = 'Disconnected';
        this.prevBytesReceived = 0;
        this.prevStatsTime = null;
    }
}

// Initialize client when page loads
document.addEventListener('DOMContentLoaded', () => {
    new RTCStreamerClient();
});
