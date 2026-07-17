#ifndef ALAN_PTY_SPAWN_SUPPORT_H
#define ALAN_PTY_SPAWN_SUPPORT_H

#include <signal.h>
#include <stddef.h>

static inline void alan_reset_child_signal_state(void) {
    sigset_t empty_mask;
    sigemptyset(&empty_mask);
    sigprocmask(SIG_SETMASK, &empty_mask, NULL);

    const int signals_to_reset[] = {
        SIGHUP,
        SIGINT,
        SIGQUIT,
        SIGTERM,
        SIGTSTP,
        SIGTTIN,
        SIGTTOU,
        SIGPIPE,
    };
    struct sigaction action;
    action.sa_handler = SIG_DFL;
    sigemptyset(&action.sa_mask);
    action.sa_flags = 0;
    for (size_t index = 0;
         index < sizeof(signals_to_reset) / sizeof(signals_to_reset[0]);
         index++) {
        sigaction(signals_to_reset[index], &action, NULL);
    }
}

#endif
