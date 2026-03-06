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

            // Connect WebSocket
            const wsUrl = `ws://${window.location.host}/signal`;
            this.ws = new WebSocket(wsUrl);

            this.ws.onopen = () => {
                console.log('WebSocket connected');
                this.sendMessage({ type: 'watch' });
            };

            this.ws.onmessage = async (event) => {
                const message = JSON.parse(event.data);
                await this.handleSignalingMessage(message);
            };

            this.ws.onerror = (error) => {
                console.error('WebSocket error:', error);
                this.updateStatus('Connection error', 'error');
                this.showLoading(false);
                this.connectBtn.disabled = false;
            };

            this.ws.onclose = () => {
                console.log('WebSocket closed');
                this.updateStatus('Disconnected', 'disconnected');
                this.showLoading(false);
                this.connectBtn.disabled = false;
                this.disconnectBtn.disabled = true;
                this.stopStats();
            };

        } catch (error) {
            console.error('Connection error:', error);
            this.updateStatus('Failed to connect', 'error');
            this.showLoading(false);
            this.connectBtn.disabled = false;
        }
    }

    async handleSignalingMessage(message) {
        console.log('Received signaling message:', message.type);

        switch (message.type) {
            case 'offer':
                await this.handleOffer(message.sdp);
                break;
            case 'ice-candidate':
                await this.handleIceCandidate(message.candidate);
                break;
            case 'error':
                console.error('Server error:', message.message);
                this.updateStatus(`Error: ${message.message}`, 'error');
                break;
        }
    }

    async handleOffer(sdp) {
        console.log('Handling offer');

        // Create RTCPeerConnection
        const config = {
            iceServers: [
                { urls: 'stun:stun.l.google.com:19302' }
            ]
        };

        this.pc = new RTCPeerConnection(config);

        // Handle incoming tracks
        this.pc.ontrack = (event) => {
            console.log('Received track:', event.track.kind);
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
                console.log('Sending ICE candidate');
                this.sendMessage({
                    type: 'ice-candidate',
                    candidate: event.candidate.candidate
                });
            }
        };

        // Handle connection state changes
        this.pc.onconnectionstatechange = () => {
            console.log('Connection state:', this.pc.connectionState);
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

        console.log('Answer sent');
    }

    async handleIceCandidate(candidate) {
        if (this.pc) {
            try {
                await this.pc.addIceCandidate(new RTCIceCandidate({
                    candidate: candidate
                }));
                console.log('ICE candidate added');
            } catch (error) {
                console.error('Error adding ICE candidate:', error);
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
        this.statsInterval = setInterval(async () => {
            if (!this.pc) return;

            try {
                const stats = await this.pc.getStats();
                let bytesReceived = 0;
                let packetsLost = 0;

                stats.forEach(report => {
                    if (report.type === 'inbound-rtp' && report.kind === 'video') {
                        bytesReceived = report.bytesReceived || 0;
                        packetsLost = report.packetsLost || 0;
                    }
                });

                // Calculate bitrate (rough estimate)
                const bitrate = Math.round((bytesReceived * 8) / 1000); // kbps
                this.bitrateElement.textContent = `${bitrate} kbps`;
                this.packetsLostElement.textContent = packetsLost;

            } catch (error) {
                console.error('Error getting stats:', error);
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
        this.latencyElement.textContent = '0 ms';
        this.connStatusElement.textContent = 'Disconnected';
    }
}

// Initialize client when page loads
document.addEventListener('DOMContentLoaded', () => {
    const client = new RTCStreamerClient();
    console.log('RTC Streamer Client initialized');
});
