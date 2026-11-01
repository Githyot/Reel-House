// iOS only (Objective-C++, compiled by the IOS branch of CMakeLists.txt): the safe area of the app's window (notch,
// home indicator) and the idle timer, for Platform (platform.cpp).
#import <UIKit/UIKit.h>
#include <QMarginsF>

static UIWindow *reelhouseKeyWindow() {
    for (UIScene *scene in UIApplication.sharedApplication.connectedScenes) {
        if (![scene isKindOfClass:[UIWindowScene class]]) continue;
        for (UIWindow *w in ((UIWindowScene *)scene).windows)
            if (w.isKeyWindow) return w;
        if (((UIWindowScene *)scene).windows.count) return ((UIWindowScene *)scene).windows.firstObject;
    }
    return nil;
}

QMarginsF reelhouseIosSafeArea() {
    UIWindow *w = reelhouseKeyWindow();
    if (!w) return QMarginsF();
    const UIEdgeInsets i = w.safeAreaInsets;  // points = Qt's logical pixels
    return QMarginsF(i.left, i.top, i.right, i.bottom);
}

void reelhouseIosKeepScreenOn(bool on) {
    UIApplication.sharedApplication.idleTimerDisabled = on ? YES : NO;
}
