#!/usr/bin/env python3
"""Capture a screenshot of the gpuidart window under Xvfb using mss."""
import sys
import time
import mss
import mss.tools

def main():
    out_path = sys.argv[1] if len(sys.argv) > 1 else '/tmp/shot.png'
    delay = float(sys.argv[2]) if len(sys.argv) > 2 else 2.0
    print(f'waiting {delay}s for window...', flush=True)
    time.sleep(delay)
    with mss.mss() as sct:
        monitor = sct.monitors[0]
        shot = sct.grab(monitor)
        mss.tools.to_png(shot.rgb, shot.size, output=out_path)
    print(f'saved {out_path} ({shot.width}x{shot.height})', flush=True)

if __name__ == '__main__':
    main()
