#include <errno.h>
#include <grp.h>
#include <sys/ioctl.h>
#include <sys/types.h>
#include <unistd.h>
#include <util.h>

#include "AlanPtySpawnSupport.h"

int alan_darwin_pty_spawn_as_user(
    const char *executable_path,
    char *const argv[],
    char *const envp[],
    const char *working_directory,
    const char *account_name,
    uid_t uid,
    gid_t gid,
    unsigned short rows,
    unsigned short columns,
    int *master_fd_out,
    pid_t *pid_out
) {
    if (executable_path == NULL || argv == NULL || envp == NULL ||
        working_directory == NULL || account_name == NULL ||
        master_fd_out == NULL || pid_out == NULL) {
        return EINVAL;
    }

    struct winsize size = {
        .ws_row = rows,
        .ws_col = columns,
        .ws_xpixel = 0,
        .ws_ypixel = 0,
    };

    int master_fd = -1;
    int slave_fd = -1;
    if (openpty(&master_fd, &slave_fd, NULL, NULL, &size) != 0) {
        return errno;
    }

    pid_t pid = fork();
    if (pid < 0) {
        int saved_errno = errno;
        close(master_fd);
        close(slave_fd);
        return saved_errno;
    }

    if (pid == 0) {
        close(master_fd);
        alan_reset_child_signal_state();
        if (setsid() < 0) {
            _exit(126);
        }
        if (ioctl(slave_fd, TIOCSCTTY, 0) < 0) {
            _exit(126);
        }
        if (initgroups(account_name, gid) != 0) {
            _exit(126);
        }
        if (setgid(gid) != 0 || setuid(uid) != 0) {
            _exit(126);
        }
        if (chdir(working_directory) != 0) {
            _exit(126);
        }
        if (dup2(slave_fd, STDIN_FILENO) < 0 ||
            dup2(slave_fd, STDOUT_FILENO) < 0 ||
            dup2(slave_fd, STDERR_FILENO) < 0) {
            _exit(126);
        }
        if (slave_fd > STDERR_FILENO) {
            close(slave_fd);
        }
        execve(executable_path, argv, envp);
        _exit(127);
    }

    close(slave_fd);
    *master_fd_out = master_fd;
    *pid_out = pid;
    return 0;
}
