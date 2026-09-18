/* TEST FIXTURE ONLY. Never installed or linked into AIRWAV. No RF hardware. */
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <stdatomic.h>
#include <time.h>
static int mode;
struct device { uint32_t rate, center; int ppm, test, mode; atomic_int cancel; };
void airwav_test_mode(int m) { mode = m; }
uint32_t rtlsdr_get_device_count(void) { return mode == 8 ? 0 : 1; }
int rtlsdr_get_device_usb_strings(uint32_t i, char *m, char *p, char *s) {
    (void)i; strcpy(m,"RTLSDRBlog"); strcpy(p, mode == 1 ? "Blog V3" : "Blog V4"); strcpy(s,"TEST-ONLY"); return 0;
}
int rtlsdr_get_usb_strings(void *d, char *m, char *p, char *s) { (void)d; return rtlsdr_get_device_usb_strings(0,m,p,s); }
int rtlsdr_open(void **out, uint32_t i) { (void)i; struct device *d=calloc(1,sizeof(*d)); d->mode=mode; *out=d; return 0; }
int rtlsdr_close(void *d) { free(d); return 0; }
int rtlsdr_get_tuner_type(void *d) { return ((struct device*)d)->mode==2 ? 5 : 6; }
int rtlsdr_get_xtal_freq(void *d,uint32_t *r,uint32_t *t) { *r=28800000; *t=((struct device*)d)->mode==3 ? 16000000 : 28800000; return 0; }
int rtlsdr_set_sample_rate(void *p,uint32_t r) { ((struct device*)p)->rate=r; return 0; }
uint32_t rtlsdr_get_sample_rate(void *p) { return ((struct device*)p)->rate; }
int rtlsdr_set_center_freq(void *p,uint32_t c) { ((struct device*)p)->center=c; return 0; }
uint32_t rtlsdr_get_center_freq(void *p) { return ((struct device*)p)->center; }
int rtlsdr_set_freq_correction(void *p,int c) { struct device*d=p; if(d->ppm==c) return -2; d->ppm=c; return 0; }
int rtlsdr_get_freq_correction(void *p) { return ((struct device*)p)->ppm; }
int rtlsdr_set_tuner_gain_mode(void *p,int g) { (void)p;(void)g;return 0; }
int rtlsdr_set_tuner_gain(void *p,int g) { (void)p;(void)g;return 0; }
int rtlsdr_get_tuner_gains(void *p,int *g) { (void)p;if(g){g[0]=0;g[1]=200;}return 2; }
int rtlsdr_set_bias_tee(void *p,int g) { (void)p;(void)g;return 0; }
int rtlsdr_read_eeprom(void *p,uint8_t *b,uint8_t o,uint16_t n) { (void)o;(void)n;*b=((struct device*)p)->mode==7 ? 0 : 2;return 1; }
int rtlsdr_set_agc_mode(void *p,int g) { (void)p;(void)g;return 0; }
int rtlsdr_reset_buffer(void *p) { (void)p;return 0; }
int rtlsdr_set_testmode(void *p,int g) { ((struct device*)p)->test=g;return 0; }
int rtlsdr_cancel_async(void *p) { atomic_store(&((struct device*)p)->cancel,1);return 0; }
int rtlsdr_read_async(void *p,void(*cb)(uint8_t*,uint32_t,void*),void *ctx,uint32_t count,uint32_t len) {
    (void)count;struct device*d=p;uint8_t *b=malloc(len);unsigned char v=0;atomic_store(&d->cancel,0);
    struct timespec pause={0,1000000};
    for(int block=0;block<60 || d->mode==4;block++) {
        if(atomic_load(&d->cancel)) break;
        if(d->mode==5 && block==4) { free(b);return -4; }
        if(d->mode==6 && block==4) v+=7;
        for(uint32_t i=0;i<len;i++) b[i]=v++;
        cb(b,len,ctx);nanosleep(&pause,NULL);
    }
    free(b);return 0;
}
