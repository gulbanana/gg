export function isMacOS(): boolean {
    if (typeof navigator !== 'undefined') {
        if ('userAgentData' in navigator && (navigator as any).userAgentData?.platform) {
            return (navigator as any).userAgentData.platform === 'macOS';
        }
        return navigator.platform?.toLowerCase().includes('mac') ?? false;
    }
    return false;
}

export function getAltKeyName(): string {
    return isMacOS() ? 'option' : 'alt';
}
