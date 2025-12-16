#include <stdio.h>
#include <stdlib.h>
#include <linux/input.h>
#include <unistd.h>
#include <string.h>
#include <syslog.h>
#include <glob.h>

//helper function to find sway socket (used for querying focused window)
char* find_sway_socket() {
    glob_t globbuf;
    char pattern[256];
    
    // Get the user's UID
    uid_t uid = getuid();
    snprintf(pattern, sizeof(pattern), "/run/user/%d/sway-ipc.*.sock", uid);
    
    // Find matching socket files
    if (glob(pattern, 0, NULL, &globbuf) == 0 && globbuf.gl_pathc > 0) {
        char *result = strdup(globbuf.gl_pathv[0]);
        globfree(&globbuf);
        return result;
    }
    
    globfree(&globbuf);
    return NULL;
}

//helper function that queries sway to determine focused window
int is_runelite_focused() {
    char *swaysock = find_sway_socket();
    if (swaysock == NULL) {
        return 0;
    }
    
    char cmd[512];
    snprintf(cmd, sizeof(cmd), 
             "SWAYSOCK=%s swaymsg -t get_tree 2>/dev/null | jq -r '.. | select(.focused? == true) | .app_id // .window_properties.class' 2>/dev/null",
             swaysock);
    
    FILE *fp = popen(cmd, "r");
    if (fp == NULL) return 0;
    
    char output[256];
    int is_focused = 0;
    
    if (fgets(output, sizeof(output), fp) != NULL) {
        if (strstr(output, "net-runelite-client-RuneLite") != NULL) {
            is_focused = 1;
        }
    }
    
    pclose(fp);
    return is_focused;
}

//transforms mouse events passed in from intercept
int main(void) {
    setbuf(stdin, NULL), setbuf(stdout, NULL);

    openlog("runelite-remapping", LOG_PID, LOG_USER);

    struct input_event event;
    while (fread(&event, sizeof(event), 1, stdin) == 1) {
        if (event.type == EV_KEY && is_runelite_focused()){
            switch(event.code){
                case BTN_SIDE:
                    syslog(LOG_INFO, "Remapping to ESC");
                    event.code = KEY_ESC;
                    break;
                case BTN_EXTRA:
                    syslog(LOG_INFO, "Remapping to LeftShift");
                    event.code = KEY_LEFTSHIFT;
                    break;
            }
        }

        fwrite(&event, sizeof(event), 1, stdout);
    }
}
