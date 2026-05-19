import cv2
import tkinter as tk
from tkinter import filedialog, simpledialog
import sys
import time
import ctypes
from pathlib import Path

# Note: This tester is now a standalone utility for video playback.
# ROI and ImageDB management should be handled via the Rust application's debug UI
# or separate tools, as the Python core has been decommissioned.

ctypes.windll.user32.ShowCursor.argtypes = [ctypes.c_bool]

JACKET_SAVE_DIR = Path(__file__).parent / "jackets"
JACKET_SAVE_DIR.mkdir(exist_ok=True)

class BorderlessTester:
    VK_ESCAPE = 0x1B
    VK_SPACE = 0x20
    VK_LEFT = 0x25
    VK_RIGHT = 0x27

    def __init__(self):
        self.win_name = "DJMAX RESPECT V"

        self.root = tk.Tk()
        self.root.withdraw()
        self.video_path = filedialog.askopenfilename(
            title="테스트할 영상 선택",
            filetypes=[("Video files", "*.mp4 *.avi *.mkv *.mov"), ("All files", "*.*")]
        )
        
        if not self.video_path:
            print("[Tester] 영상이 선택되지 않았습니다.")
            sys.exit(0)

        self.cap = cv2.VideoCapture(self.video_path)
        if not self.cap.isOpened():
            print(f"[Tester] 영상을 열 수 없습니다: {self.video_path}")
            sys.exit(1)

        self.width = int(self.cap.get(cv2.CAP_PROP_FRAME_WIDTH))
        self.height = int(self.cap.get(cv2.CAP_PROP_FRAME_HEIGHT))
        self.fps = self.cap.get(cv2.CAP_PROP_FPS)
        if self.fps <= 0: self.fps = 60.0
        
        self.frame_interval = 1.0 / self.fps
        self.last_frame_ts = 0.0
        
        self.is_paused = False
        self.current_frame = None
        
        # Keyboard states
        self.key_state = {}
        
        print(f"[Tester] 창 이름: {self.win_name}")
        print(f"[Tester] 해상도: {self.width}x{self.height} @ {self.fps:.2f} FPS")
        print("[Tester] 단축키: Space(정지/재생), Left/Right(5초 이동), ESC(종료)")

        self.run()

    def _is_pressed_once(self, vk: int) -> bool:
        is_down = bool(ctypes.windll.user32.GetAsyncKeyState(vk) & 0x8000)
        was_down = self.key_state.get(vk, False)
        self.key_state[vk] = is_down
        return is_down and not was_down

    def _handle_hotkeys(self) -> bool:
        if self._is_pressed_once(self.VK_ESCAPE):
            return True
        if self._is_pressed_once(self.VK_SPACE):
            self.is_paused = not self.is_paused
            print(f"[Tester] 일시정지: {'ON' if self.is_paused else 'OFF'}")
        if self._is_pressed_once(self.VK_LEFT):
            pos = self.cap.get(cv2.CAP_PROP_POS_MSEC)
            self.cap.set(cv2.CAP_PROP_POS_MSEC, max(0, pos - 5000))
            self.last_frame_ts = 0
        if self._is_pressed_once(self.VK_RIGHT):
            pos = self.cap.get(cv2.CAP_PROP_POS_MSEC)
            self.cap.set(cv2.CAP_PROP_POS_MSEC, pos + 5000)
            self.last_frame_ts = 0
        return False

    def run(self):
        cv2.namedWindow(self.win_name, cv2.WINDOW_NORMAL)
        # Set to full screen or borderless style if needed
        cv2.setWindowProperty(self.win_name, cv2.WND_PROP_FULLSCREEN, cv2.WINDOW_FULLSCREEN)

        while True:
            now = time.perf_counter()
            if not self.is_paused:
                if now - self.last_frame_ts >= self.frame_interval:
                    ret, frame = self.cap.read()
                    if not ret:
                        self.cap.set(cv2.CAP_PROP_POS_FRAMES, 0)
                        continue
                    self.current_frame = frame
                    self.last_frame_ts = now
                    cv2.imshow(self.win_name, self.current_frame)

            if cv2.waitKey(1) & 0xFF == 27:
                break
            
            if self._handle_hotkeys():
                break
                
            if cv2.getWindowProperty(self.win_name, cv2.WND_PROP_VISIBLE) < 1:
                break

        self.cap.release()
        cv2.destroyAllWindows()

if __name__ == "__main__":
    BorderlessTester()
