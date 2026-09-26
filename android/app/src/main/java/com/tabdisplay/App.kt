package com.tabdisplay

import android.app.Application
import io.sentry.android.core.SentryAndroid

class App : Application() {
    override fun onCreate() {
        super.onCreate()
        if (BuildConfig.SENTRY_DSN.isEmpty()) return // crash reporting is off unless a DSN was built in
        SentryAndroid.init(this) { options ->
            options.dsn = BuildConfig.SENTRY_DSN
            options.isSendDefaultPii = false
            options.beforeSend = io.sentry.SentryOptions.BeforeSendCallback { event, _ ->
                event.user = null
                event.serverName = null
                event
            }
        }
    }
}
