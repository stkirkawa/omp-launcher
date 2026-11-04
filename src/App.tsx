import { invoke } from "@tauri-apps/api";
import {
  appWindow,
  LogicalPosition,
  LogicalSize,
  type PhysicalSize,
} from "@tauri-apps/api/window";
import {
  lazy,
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { StyleSheet, View } from "react-native";
import { DEBUG_MODE, IN_GAME, IN_GAME_PROCESS_ID } from "./constants/app";
import LoadingScreen from "./containers/LoadingScreen";
import WindowTitleBar from "./containers/WindowTitleBar";
import { changeLanguage } from "./locales";
import { useGenericPersistentState } from "./states/genericStates";
import { useTheme } from "./states/theme";
import { throttle } from "./utils/debounce";
import { checkIfProcessAlive, fetchUpdateInfo } from "./utils/helpers";
import PerformanceMonitor from "./utils/performance";
import { sc } from "./utils/sizeScaler";

const LOADING_WINDOW_SIZE = new LogicalSize(250, 300);
const DEFAULT_WINDOW_SIZE = new LogicalSize(1000, 700);
const BROWSER_PREVIEW = new URLSearchParams(window.location.search).has(
  "preview"
);

// Lazy load heavy components for better initial load time
const MainView = lazy(() => import("./containers/MainBody"));
const MessageBox = lazy(() => import("./containers/MessageBox"));
const Notification = lazy(() => import("./containers/Notification"));
const SettingsModal = lazy(() => import("./containers/Settings"));

const App = memo(() => {
  const [loading, setLoading] = useState(!IN_GAME && !BROWSER_PREVIEW);
  const [maximized, setMaximized] = useState(false);
  const { theme } = useTheme();
  const { language } = useGenericPersistentState();
  const windowSize = useRef<PhysicalSize>();
  const mainWindowSize = useRef<LogicalSize>();
  const processCheckInterval = useRef<NodeJS.Timeout>();

  const windowResizeListener = useCallback(
    throttle(async ({ payload }: { payload: PhysicalSize }) => {
      const endTimer = PerformanceMonitor.time("window-resize");

      try {
        const hasChanged =
          payload.width !== windowSize.current?.width ||
          payload.height !== windowSize.current?.height;

        if (hasChanged) {
          const isMaximized = await appWindow.isMaximized();
          setMaximized(isMaximized);
          windowSize.current = payload;
        }
      } finally {
        endTimer();
      }
    }, 100), // Increased throttle delay for better performance
    []
  );

  const initializeApp = useCallback(async () => {
    const endTimer = PerformanceMonitor.time("app-initialization");

    try {
      const [innerSize, scaleFactor] = await Promise.all([
        appWindow.innerSize(),
        appWindow.scaleFactor(),
      ]);

      mainWindowSize.current = innerSize.toLogical(scaleFactor);

      // If the window is already at the loading size (e.g. after a reload),
      // do not capture it as the main window size.
      if (
        mainWindowSize.current.width === LOADING_WINDOW_SIZE.width &&
        mainWindowSize.current.height === LOADING_WINDOW_SIZE.height
      ) {
        mainWindowSize.current = DEFAULT_WINDOW_SIZE;
      }

      // Set window attributes for loading screen
      await Promise.all([
        appWindow.setSize(LOADING_WINDOW_SIZE),
        appWindow.setResizable(false),
        appWindow.center(),
      ]);

      if (!IN_GAME) {
        await fetchUpdateInfo();
      }
    } finally {
      endTimer();
    }
  }, []);

  useEffect(() => {
    changeLanguage(language as any);
  }, [language]);

  useEffect(() => {
    if (!loading) {
      const targetSize = mainWindowSize.current || DEFAULT_WINDOW_SIZE;

      if (!BROWSER_PREVIEW) {
        Promise.all([
          appWindow.setResizable(true),
          appWindow.setSize(targetSize),
        ]);

        if (!IN_GAME) {
          appWindow.center();
        }
      }
    }
  }, [loading]);

  useEffect(() => {
    let killResizeListener: (() => void) | null = null;

    const setupListeners = async () => {
      // Optimize context menu handler
      if (!DEBUG_MODE) {
        const handleContextMenu = (event: Event) => {
          event.preventDefault();
        };
        document.addEventListener("contextmenu", handleContextMenu, {
          passive: false,
        });
      }

      killResizeListener = await appWindow.onResized(windowResizeListener);
    };

    const setupGameMonitoring = () => {
      if (IN_GAME) {
        processCheckInterval.current = setInterval(async () => {
          try {
            const isAlive = await checkIfProcessAlive(IN_GAME_PROCESS_ID);
            if (!isAlive) {
              await invoke("send_message_to_game", {
                id: IN_GAME_PROCESS_ID,
                message: "close_overlay",
              });
              setTimeout(() => appWindow.close(), 300);
            }
          } catch (error) {
            console.error("Game process check failed:", error);
          }
        }, 1000); // Reduced frequency for better performance
      }
    };

    if (!BROWSER_PREVIEW) {
      setupListeners();
      initializeApp();
    }
    setupGameMonitoring();

    if (IN_GAME) {
      setInterval(async () => {
        appWindow.setPosition(new LogicalPosition(-15000, -15000));

        const visible = await appWindow.isVisible();
        if (!visible) {
          appWindow.show();
        }
      }, 100);
    }

    return () => {
      killResizeListener?.();
      if (processCheckInterval.current) {
        clearInterval(processCheckInterval.current);
      }
    };
  }, [windowResizeListener, initializeApp]);

  const handleLoadingEnd = useCallback(async () => {
    const endTimer = PerformanceMonitor.time("loading-end");
    setLoading(false);
    endTimer();
  }, []);

  const appStyle = useMemo(
    () => [styles.app, { padding: maximized || IN_GAME ? 0 : 4 }],
    [maximized]
  );

  const appViewStyle = useMemo(
    () => [
      styles.appView,
      {
        borderRadius: maximized || IN_GAME ? 0 : 10,
        backgroundColor: theme.secondary,
      },
    ],
    [maximized, theme.secondary]
  );

  if (loading) {
    return <LoadingScreen onEnd={handleLoadingEnd} />;
  }

  return (
    <View style={appStyle} key={language}>
      <View style={appViewStyle}>
        <WindowTitleBar />
        <View style={styles.appBody}>
          <MainView />
          <SettingsModal />
          <Notification />
          <MessageBox />
        </View>
      </View>
    </View>
  );
});

App.displayName = "App";

const styles = StyleSheet.create({
  app: {
    // @ts-ignore
    height: "100vh",
    // @ts-ignore
    width: "100vw",
  },
  appView: {
    height: "100%",
    width: "100%",
    overflow: "hidden",
    shadowColor: "#000",
    shadowOffset: {
      width: 0,
      height: 0,
    },
    shadowOpacity: 0.6,
    shadowRadius: 4.65,
  },
  appBody: {
    flex: 1,
    width: "100%",
    paddingHorizontal: sc(15),
    paddingBottom: sc(15),
  },
});

export default App;
